//! Exact runner terminal settlement projected into the cumulative goal ledger.

use super::{
    GoalSettlement, ProductRunOutcome, ProductRunService, ProductRunServiceError, RunDisposition,
};

impl ProductRunService {
    pub(super) fn settle_workbench_goal(
        &self,
        record: &super::super::RunRecord,
        result: &Result<ProductRunOutcome, peritus_product_runner::ProductRunnerError>,
    ) -> Result<(), ProductRunServiceError> {
        let Some(start) = record.interaction.as_ref().and_then(|options| options.workbench.clone())
        else {
            return Ok(());
        };
        let evidence_input_generation = record
            .interaction
            .as_ref()
            .map(|options| options.incorporated)
            .filter(|revision| *revision > 0);
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
            store.settle_goal(&start, settlement, evidence_input_generation, unresolved_effects)
        })?;
        Ok(())
    }
}
