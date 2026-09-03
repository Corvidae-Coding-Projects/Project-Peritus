//! Active-loop exit facts consumed by the protected finalization boundary.

use peritus_run_settlement::SettlementCause;

use super::{ProductRunPhase, settlement};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

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
            "the configured active phase window ended with finalization time preserved".to_owned(),
            next_phase,
        )
    }

    pub(super) fn from_error(
        error: &ProductRunnerError,
        deadline: bool,
        next_phase: ProductRunPhase,
    ) -> Self {
        let cause = settlement::cause_from_error(error, deadline);
        Self::stopped(cause, format!("{}: {}", error.operation(), error.detail()), next_phase)
    }

    pub(super) const fn with_deadline(mut self, reached: bool) -> Self {
        if reached && !matches!(self.cause, SettlementCause::Completed | SettlementCause::UserWait)
        {
            self.cause = SettlementCause::Deadline;
        }
        self
    }
}

pub(super) const fn fatal(error: &ProductRunnerError) -> bool {
    matches!(
        error.kind(),
        ProductRunnerErrorKind::InvalidPrecondition | ProductRunnerErrorKind::InternalInvariant
    )
}
