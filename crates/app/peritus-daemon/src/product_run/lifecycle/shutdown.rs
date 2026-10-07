//! Product-run admission closure and complete owner settlement during daemon shutdown.

use std::{collections::BTreeSet, sync::atomic::Ordering};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

use super::super::{ProductRunService, ProductRunServiceError};
use super::replace_snapshot;
use crate::product_run::publication::{MutationDisposition, RunMutationKind};

impl ProductRunService {
    pub(crate) async fn shutdown(&self) -> Result<(), DaemonError> {
        let (owners, mut failures) = {
            let mut tasks = self.inner.tasks.lock().await;
            tasks.accepting = false;
            let mut failures = ShutdownFailures::default();
            failures.owner_failures = tasks.failed_owners;
            failures.first_detail = tasks.first_owner_failure.take();
            tasks.failed_owners = 0;
            (std::mem::take(&mut tasks.owners), failures)
        };

        if let Err(error) = self.inner.control_generation.begin_draining() {
            failures.record_state(format!(
                "the control generation could not begin draining: {error}"
            ));
        }
        self.inner.control_shutdown.cancel();
        let mut user_cancellations = BTreeSet::new();
        let mut registered_cancellations = BTreeSet::new();
        match self.inner.run_cancellations.lock() {
            Ok(cancellations) => {
                for (run_id, cancellation) in cancellations.iter() {
                    registered_cancellations.insert(*run_id);
                    if cancellation.user_requested.load(Ordering::Acquire) {
                        user_cancellations.insert(*run_id);
                    }
                    cancellation.attempt_cancelled.store(true, Ordering::Release);
                    let _ = cancellation.provider.cancel();
                    cancellation.control.cancel();
                }
            }
            Err(_) => failures.record_state("the run cancellation registry was poisoned"),
        }

        let projection_service = self.clone();
        match tokio::task::spawn_blocking(move || {
            projection_service.project_shutdown_stopping(
                &user_cancellations,
                &registered_cancellations,
            )
        })
        .await
        {
            Ok((_interrupted, projection_failures)) => {
                failures.merge(projection_failures);
            }
            Err(error) => {
                failures.record_owner(format!(
                    "shutdown stopping projection worker failed: {error}"
                ));
            }
        }
        self.await_shutdown_owners(owners, &mut failures).await;

        // Exact terminal reconciliation remains live until all original owners have returned and
        // every unresolved terminal obligation has either completed or reported a retained error.
        let mut final_user_cancellations = BTreeSet::new();
        match self.inner.run_cancellations.lock() {
            Ok(cancellations) => {
                for (run_id, cancellation) in cancellations.iter() {
                    if cancellation.user_requested.load(Ordering::Acquire) {
                        final_user_cancellations.insert(*run_id);
                    }
                }
            }
            Err(_) => failures.record_state(
                "the run cancellation registry was poisoned during settlement",
            ),
        }
        let projection_service = self.clone();
        match tokio::task::spawn_blocking(move || {
            projection_service.project_shutdown_settlement(&final_user_cancellations)
        })
        .await
        {
            Ok(projection_failures) => failures.merge(projection_failures),
            Err(error) => failures.record_owner(format!(
                "shutdown settlement projection worker failed: {error}"
            )),
        }
        self.inner.control_reconciliation.cancel();

        for detail in self.inner.publications.shutdown_and_join() {
            failures.record_owner(detail);
        }
        if let Err(error) = self
            .inner
            .control_generation
            .finish_draining(&self.inner.control_reconciliation)
        {
            failures.record_state(format!(
                "the control generation could not finish draining: {error}"
            ));
        }

        failures.finish()
    }

    async fn await_shutdown_owners(
        &self,
        owners: Vec<tokio::task::JoinHandle<Result<(), String>>>,
        failures: &mut ShutdownFailures,
    ) {
        for owner in owners {
            match owner.await {
                Ok(Ok(())) => {}
                Ok(Err(detail)) => failures.record_owner(detail),
                Err(error) => failures.record_owner(error.to_string()),
            }
        }
    }

    fn project_shutdown_stopping(
        &self,
        user_cancellations: &BTreeSet<peritus_types::RunId>,
        registered_cancellations: &BTreeSet<peritus_types::RunId>,
    ) -> (Vec<(peritus_types::RunId, bool)>, ShutdownFailures) {
        let mut failures = ShutdownFailures::default();
        let mut interrupted = Vec::new();
        let runs = match self.inner.records.read() {
            Ok(records) => records
                .iter()
                .map(|(run, record)| {
                    (
                        *run,
                        std::sync::Arc::clone(&record.cancelled),
                        record.snapshot.phase().terminal(),
                        record.user_cancelled,
                        record.control_cancellation.clone(),
                        record.provider_cancellation.clone(),
                    )
                })
                .collect::<Vec<_>>(),
            Err(_) => {
                failures.record_state("the product-run registry was poisoned");
                return (interrupted, failures);
            }
        };
        for (run_id, attempt, terminal, was_cancelled, control, provider) in runs {
            let user_cancelled = was_cancelled || user_cancellations.contains(&run_id);
            if !terminal {
                let mut input = run_id.as_bytes().to_vec();
                input.push(u8::from(user_cancelled));
                match self.mutate_run(
                    run_id,
                    Some(&attempt),
                    RunMutationKind::ShutdownStopping,
                    peritus_codec::sha256(&input),
                    MutationDisposition::DurabilityRequired,
                    |record| {
                        record.user_cancelled |= user_cancelled;
                        record.snapshot = replace_snapshot(
                            &record.snapshot,
                            record.snapshot.phase(),
                            "Stopping safely after the current effect boundary",
                            record.snapshot.summary(),
                        )?;
                        Ok(())
                    },
                ) {
                    Ok(((), ticket)) => match self.await_run_durable(ticket) {
                        Ok(()) => interrupted.push((run_id, user_cancelled)),
                        Err(error) => failures.record_persistence(error),
                    },
                    Err(error) => failures.record_state(error.describe()),
                }
            }
            attempt.store(true, Ordering::Release);
            control.cancel();
            if !registered_cancellations.contains(&run_id) {
                let _ = provider.cancel();
            }
        }
        (interrupted, failures)
    }

    fn project_shutdown_settlement(
        &self,
        user_cancellations: &BTreeSet<peritus_types::RunId>,
    ) -> ShutdownFailures {
        let mut failures = ShutdownFailures::default();
        let owned = match self.inner.run_cancellations.lock() {
            Ok(cancellations) => cancellations
                .iter()
                .filter(|(_, cancellation)| cancellation.active.load(Ordering::Acquire))
                .map(|(run, _)| *run)
                .collect::<BTreeSet<_>>(),
            Err(_) => {
                failures.record_state(
                    "the run cancellation registry was poisoned during settlement",
                );
                return failures;
            }
        };
        let unsettled = match self.inner.records.read() {
            Ok(records) => records
                .iter()
                .filter(|(_, record)| {
                    owned.contains(&record.request.run_id())
                        && (!record.snapshot.phase().terminal()
                            || !record.settlement_obligation.as_ref().is_some_and(
                                super::super::persistence::SettlementObligation::resolved,
                            ))
                })
                .map(|(run, record)| {
                    (
                        *run,
                        std::sync::Arc::clone(&record.cancelled),
                        record.user_cancelled || user_cancellations.contains(run),
                    )
                })
                .collect::<Vec<_>>(),
            Err(_) => {
                failures.record_state("the product-run registry was poisoned during settlement");
                return failures;
            }
        };
        for (run_id, attempt, user_cancelled) in unsettled {
            if let Err(detail) =
                self.settle_shutdown_interruption(run_id, &attempt, user_cancelled)
            {
                failures.record_state(format!(
                    "run {run_id:?} could not complete terminal shutdown settlement: {detail}"
                ));
            }
        }
        failures
    }
}

#[derive(Default)]
struct ShutdownFailures {
    owner_failures: usize,
    persistence_failures: usize,
    state_failures: usize,
    first_detail: Option<String>,
}

impl ShutdownFailures {
    fn merge(&mut self, other: Self) {
        self.owner_failures = self.owner_failures.saturating_add(other.owner_failures);
        self.persistence_failures =
            self.persistence_failures.saturating_add(other.persistence_failures);
        self.state_failures = self.state_failures.saturating_add(other.state_failures);
        if self.first_detail.is_none() {
            self.first_detail = other.first_detail;
        }
    }

    fn record_owner(&mut self, detail: impl Into<String>) {
        self.owner_failures = self.owner_failures.saturating_add(1);
        self.retain_first(detail);
    }

    fn record_persistence(&mut self, error: ProductRunServiceError) {
        self.persistence_failures = self.persistence_failures.saturating_add(1);
        self.retain_first(error.describe());
    }

    fn record_state(&mut self, detail: impl Into<String>) {
        self.state_failures = self.state_failures.saturating_add(1);
        self.retain_first(detail);
    }

    fn retain_first(&mut self, detail: impl Into<String>) {
        if self.first_detail.is_none() {
            self.first_detail = Some(detail.into());
        }
    }

    fn finish(self) -> Result<(), DaemonError> {
        if self.owner_failures == 0
            && self.persistence_failures == 0
            && self.state_failures == 0
        {
            return Ok(());
        }
        let code = if self.owner_failures > 0 || self.state_failures > 0 {
            DaemonErrorCode::UncleanShutdown
        } else {
            DaemonErrorCode::Storage
        };
        let detail = format!(
            "{} owner join failure(s), {} durable write failure(s), and {} state failure(s). First failure: {}",
            self.owner_failures,
            self.persistence_failures,
            self.state_failures,
            self.first_detail.as_deref().unwrap_or("unavailable"),
        );
        Err(DaemonError::new(
            code,
            DaemonRecovery::Reconcile,
            "settle product-run shutdown",
            detail,
        ))
    }
}
