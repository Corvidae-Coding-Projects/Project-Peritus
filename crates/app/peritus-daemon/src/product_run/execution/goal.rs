//! Exact runner terminal settlement projected into the cumulative goal ledger.

use peritus_product_runner::control::GoalState;

use super::{
    GoalSettlement, ProductRunOutcome, ProductRunService, ProductRunServiceError, RunDisposition,
};

impl ProductRunService {
    pub(super) fn settle_workbench_goal(
        &self,
        record: &super::super::RunRecord,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<Option<&'static str>, ProductRunServiceError> {
        let start = record.interaction.workbench.clone();
        let evidence_input_generation =
            (record.interaction.incorporated > 0).then_some(record.interaction.incorporated);
        let (settlement, unresolved_effects) = match result {
            Ok(outcome) => {
                let settlement = match outcome.settlement().disposition() {
                    RunDisposition::Accepted => GoalSettlement::Accepted,
                    RunDisposition::WaitingForUser => GoalSettlement::WaitingForUser,
                    RunDisposition::RecoveryRequired => GoalSettlement::RecoveryRequired,
                    RunDisposition::Cancelled => GoalSettlement::Cancelled,
                    RunDisposition::CandidateAvailable | RunDisposition::FailedNoCandidate => {
                        GoalSettlement::Failed
                    }
                };
                let unresolved = outcome.settlement().disposition() != RunDisposition::Accepted
                    || outcome.candidate().is_none()
                    || outcome.settlement().checkpoint().is_none()
                    || !outcome.remaining_work().is_empty();
                (settlement, unresolved)
            }
            Err(error)
                if error.kind() == peritus_product_runner::ProductRunnerErrorKind::Cancelled =>
            {
                (GoalSettlement::Cancelled, true)
            }
            Err(_) => (GoalSettlement::Failed, true),
        };
        self.with_controls(false, |store| {
            store.settle_goal(&start, settlement, evidence_input_generation, unresolved_effects)?;
            let record = store.load(start.conversation())?;
            Ok(record.as_ref().and_then(|record| record.goal()).and_then(|goal| match goal.state() {
                GoalState::Paused => Some("Paused — completed effects retained; /resume continues"),
                GoalState::BudgetReached => Some("Budget reached — completed effects retained; review /budget before resuming"),
                _ => None,
            }))
        }).map_err(Into::into)
    }
}

impl ProductRunService {
    pub(super) async fn with_goal_clock<F>(
        &self,
        run: peritus_types::RunId,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        provider_cancellation: peritus_provider_core::CancellationToken,
        execution: F,
    ) -> Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>
    where
        F: Future<Output = Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>>,
    {
        let started = std::time::Instant::now();
        tokio::pin!(execution);
        let mut stopped = false;
        let mut tick_millis = 100;
        loop {
            tokio::select! {
                result = &mut execution => {
                    if !stopped
                        && let Err(error) = self.observe_goal_clock(run, started.elapsed()) {
                        self.record_goal_clock_failure(run, &error);
                    }
                    return result;
                }
                () = tokio::time::sleep(std::time::Duration::from_millis(tick_millis)), if !stopped => {
                    match self.observe_goal_clock(run, started.elapsed()) {
                        Err(error) => {
                            self.record_goal_clock_failure(run, &error);
                            cancelled.store(true, std::sync::atomic::Ordering::Release);
                            let _ = provider_cancellation.cancel();
                            stopped = true;
                        }
                        Ok(Some(0)) => {
                            cancelled.store(true, std::sync::atomic::Ordering::Release);
                            let _ = provider_cancellation.cancel();
                            stopped = true;
                        }
                        Ok(Some(remaining)) => tick_millis = remaining.min(1000),
                        Ok(None) => stopped = true,
                    }
                }
            }
        }
    }

    fn record_goal_clock_failure(&self, run: peritus_types::RunId, error: &ProductRunServiceError) {
        if let Ok(records) = self.inner.records.read()
            && let Some(record) = records.get(&run)
        {
            let options = &record.interaction;
            options.record_persistence_failure(format!(
                "Could not save goal progress: {}",
                error.describe()
            ));
            options.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    fn observe_goal_clock(
        &self,
        run: peritus_types::RunId,
        elapsed: std::time::Duration,
    ) -> Result<Option<u64>, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
        let start = record.interaction.workbench.clone();
        let progress = record.progress.clone();
        drop(records);
        self.with_controls(false, |store| {
            store.observe_goal_progress(
                &start,
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                progress.retries,
                progress.provider_failovers,
                progress.compactions,
                progress.workspace_bytes,
                progress.workspace_growth_bytes,
                progress.peak_rss_bytes,
            )?;
            store.goal_remaining_active_millis(&start)
        })
        .map_err(Into::into)
    }
}
