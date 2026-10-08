//! Active-loop exit facts consumed by the protected finalization boundary.

use peritus_run_settlement::SettlementCause;

use super::{ProductRunPhase, settlement};
use crate::ProductRunnerError;

pub(super) struct ActiveExit {
    pub(super) cause: SettlementCause,
    pub(super) question: Option<(String, u64)>,
    pub(super) detail: Option<String>,
    pub(super) next_phase: ProductRunPhase,
}

impl ActiveExit {
    pub(super) const fn completed() -> Self {
        Self {
            cause: SettlementCause::Completed,
            question: None,
            detail: None,
            next_phase: ProductRunPhase::Finalizing,
        }
    }

    pub(super) const fn waiting(
        question: String,
        revision: u64,
        next_phase: ProductRunPhase,
    ) -> Self {
        Self {
            cause: SettlementCause::UserWait,
            question: Some((question, revision)),
            detail: None,
            next_phase,
        }
    }

    pub(super) const fn stopped(
        cause: SettlementCause,
        detail: String,
        next_phase: ProductRunPhase,
    ) -> Self {
        Self { cause, question: None, detail: Some(detail), next_phase }
    }

    pub(super) fn deadline(next_phase: ProductRunPhase) -> Self {
        Self::stopped(
            SettlementCause::Deadline,
            "the caller-selected run horizon expired; retained work remains available".to_owned(),
            next_phase,
        )
    }

    pub(super) fn from_error(
        error: &ProductRunnerError,
        next_phase: ProductRunPhase,
    ) -> Self {
        let cause = settlement::cause_from_error(error, false);
        Self::stopped(cause, error.settlement_detail(), next_phase)
    }

    pub(super) fn retain_finalization_failure(&mut self, error: &ProductRunnerError) {
        if self.cause != SettlementCause::Cancellation {
            self.cause = SettlementCause::Recovery;
        }
        settlement::append_detail(&mut self.detail, error);
    }
}
