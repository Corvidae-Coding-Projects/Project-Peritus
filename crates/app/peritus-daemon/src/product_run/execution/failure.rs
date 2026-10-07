//! Terminal projection when an owned execution or publication worker stops unexpectedly.

use peritus_app_protocol::ProductRunPhase;
use peritus_types::RunId;
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crate::product_run::persistence::ObligationFirstCause;
use crate::product_run::publication::{MutationDisposition, RunMutationKind};
use crate::product_run::snapshot::replace_snapshot;
use crate::product_run::{ProductRunService, ProductRunServiceError};

const WORKER_FAILURE_PERSIST_RETRY: Duration = Duration::from_millis(50);

enum RetainedWorkerFailure {
    Ready {
        record: super::super::RunRecord,
        binding: peritus_product_runner::control::ControlOperation,
        reply: String,
        expected_input_generation: u64,
    },
    Stop,
}

impl ProductRunService {
    pub(super) async fn finish_worker_failure(
        &self,
        run_id: RunId,
        attempt: Arc<AtomicBool>,
        operation: &'static str,
        detail: String,
    ) {
        let service = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _ = service.record_terminal_failure(
                run_id,
                &attempt,
                operation,
                &detail,
                RunMutationKind::ExecutionFailure,
                ObligationFirstCause::WorkerFailure,
            );
        })
        .await;
    }

    pub(super) fn settle_shutdown_interruption(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        user_cancelled: bool,
    ) -> Result<(), String> {
        let detail = if user_cancelled {
            "user cancellation was retained while the daemon drained its owned run"
        } else {
            "daemon shutdown interrupted the owned run before terminal settlement"
        };
        self.record_terminal_failure(
            run_id,
            attempt,
            "settle daemon shutdown",
            detail,
            RunMutationKind::ShutdownSettlement,
            if user_cancelled {
                ObligationFirstCause::UserCancellation
            } else {
                ObligationFirstCause::DaemonShutdown
            },
        )?;
        self.retire_run_cancellation(run_id, attempt);
        Ok(())
    }

    fn record_terminal_failure(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        operation: &'static str,
        detail: &str,
        mutation_kind: RunMutationKind,
        fallback_cause: ObligationFirstCause,
    ) -> Result<(), String> {
        let existing_obligation = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable.describe())?
            .get(&run_id)
            .filter(|record| Arc::ptr_eq(&record.cancelled, attempt))
            .and_then(|record| record.settlement_obligation.as_ref())
            .map(|obligation| obligation.resolved());
        if let Some(resolved) = existing_obligation {
            if !resolved {
                self.recover_terminal_obligations()
                    .map_err(|error| error.describe())?;
            }
            return Ok(());
        }
        let mut reported = false;
        let RetainedWorkerFailure::Ready {
            record,
            binding,
            reply,
            expected_input_generation,
        } = loop {
            match self.try_retain_worker_failure(
                run_id,
                attempt,
                operation,
                detail,
                mutation_kind,
            ) {
                Ok(retained) => break retained,
                Err(error) => {
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: worker-failure persistence is retrying under its retained owner: {}",
                            error.describe(),
                        ));
                        reported = true;
                    }
                    if self.inner.control_shutdown.is_cancelled()
                        || self.inner.control_reconciliation.is_cancelled()
                    {
                        return Err(error.describe());
                    }
                    std::thread::sleep(WORKER_FAILURE_PERSIST_RETRY);
                }
            }
        } else {
            return Ok(());
        };
        let prepared_goal = match self.prepare_recovery_workbench_goal(&record) {
            Ok(prepared) => prepared,
            Err(error) => return Err(error.describe()),
        };
        let prepared_reply = match self.prepare_public_reply(
            run_id,
            attempt,
            &binding,
            &reply,
            expected_input_generation,
        ) {
            Ok(prepared) => prepared,
            Err(error) => return Err(error.describe()),
        };
        let reply_expected_revision = prepared_reply.expected_revision().unwrap_or(0);
        let obligation = match self.retain_terminal_obligation(
            run_id,
            attempt,
            &binding,
            &reply,
            prepared_reply.reference(),
            reply_expected_revision,
            &prepared_goal,
            if record.user_cancelled {
                ObligationFirstCause::UserCancellation
            } else {
                fallback_cause
            },
        ) {
            Ok(obligation) => obligation,
            Err(error) => return Err(error.describe()),
        };
        let status = match self.commit_workbench_goal(
            prepared_goal,
            false,
            (expected_input_generation > 0).then_some(expected_input_generation),
        ) {
            Ok(status) => status,
            Err(error) => return Err(error.describe()),
        };
        if self
            .retain_goal_obligation_progress(run_id, attempt, obligation)
            .is_err()
        {
            return Err("goal settlement progress could not be retained".to_owned());
        }
        let published = self.publish_prepared_public_reply(
            run_id,
            attempt,
            prepared_reply,
            &reply,
            expected_input_generation,
        );
        self.retain_publication_result(
            run_id,
            attempt,
            published,
            None,
            obligation,
            status,
        )?;
        if published {
            Ok(())
        } else {
            Err("terminal public reply remains pending durable recovery".to_owned())
        }
    }

    fn try_retain_worker_failure(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
        operation: &'static str,
        detail: &str,
        mutation_kind: RunMutationKind,
    ) -> Result<RetainedWorkerFailure, ProductRunServiceError> {
        let mut input = operation.as_bytes().to_vec();
        input.extend_from_slice(detail.as_bytes());
        let requested = self.user_cancellation_requested(run_id);
        let (retained, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            mutation_kind,
            peritus_codec::sha256(&input),
            MutationDisposition::DurabilityRequired,
            |record| {
        if record.snapshot.phase() == ProductRunPhase::Complete
            && record
                .settlement_obligation
                .as_ref()
                .is_some_and(crate::product_run::persistence::SettlementObligation::resolved)
        {
            return Ok(RetainedWorkerFailure::Stop);
        }
        let user_cancelled = record.user_cancelled || requested;
        let (phase, status, summary, reply) = if user_cancelled {
            (
                ProductRunPhase::Cancelled,
                "Run cancelled",
                "Cancellation interrupted the product-run worker before terminal publication.",
                "Cancelled before terminal settlement completed".to_owned(),
            )
        } else {
            (
                ProductRunPhase::RecoveryRequired,
                "Product-run worker stopped before terminal publication",
                "The owned worker stopped unexpectedly; explicit retry is required before continuing.",
                format!("{operation} failed: {detail}. Explicit retry is required."),
            )
        };
        if let Ok(snapshot) = replace_snapshot(&record.snapshot, phase, status, summary) {
            record.snapshot = snapshot;
        }
        if user_cancelled {
            record.user_cancelled = true;
        }
        let binding = record.interaction.workbench.clone();
        let expected_input_generation = record.interaction.incorporated;
        Ok(RetainedWorkerFailure::Ready {
            record: record.clone(),
            binding,
            reply,
            expected_input_generation,
        })
            },
        )?;
        self.await_run_durable(ticket)?;
        Ok(retained)
    }
}
