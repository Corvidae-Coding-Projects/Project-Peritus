//! Independently bounded E0 accounting counters.

use crate::{
    OrchestratorBinding, OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction,
};

/// Independently accounted bounded E0 work and retention counters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrchestratorCounters {
    pub(super) revisions: u16,
    pub(super) writer_cycles: u16,
    pub(super) fixer_cycles: u16,
    pub(super) gate_cycles: u16,
    pub(super) review_cycles: u16,
    pub(super) handoffs: u16,
    pub(super) child_directives: u16,
    pub(super) retained_observations: u16,
    pub(super) cancellation_reconciliations: u16,
}

impl OrchestratorCounters {
    pub(crate) const fn genesis() -> Self {
        Self {
            revisions: 1,
            writer_cycles: 0,
            fixer_cycles: 0,
            gate_cycles: 0,
            review_cycles: 0,
            handoffs: 1,
            child_directives: 0,
            retained_observations: 0,
            cancellation_reconciliations: 0,
        }
    }

    #[allow(clippy::too_many_arguments, reason = "independent wire counters remain explicit")]
    pub(crate) const fn from_wire(
        revisions: u16,
        writer_cycles: u16,
        fixer_cycles: u16,
        gate_cycles: u16,
        review_cycles: u16,
        handoffs: u16,
        child_directives: u16,
        retained_observations: u16,
        cancellation_reconciliations: u16,
    ) -> Self {
        Self {
            revisions,
            writer_cycles,
            fixer_cycles,
            gate_cycles,
            review_cycles,
            handoffs,
            child_directives,
            retained_observations,
            cancellation_reconciliations,
        }
    }

    pub(crate) fn validate(self, binding: &OrchestratorBinding) -> Result<(), OrchestratorError> {
        let limits = binding.limits();
        let bounded = [
            self.revisions > 0,
            self.revisions <= limits.revisions(),
            self.writer_cycles <= limits.writer_cycles(),
            self.fixer_cycles <= limits.fixer_cycles(),
            self.gate_cycles <= binding.effective_gate_cycles(),
            self.review_cycles <= binding.effective_review_cycles(),
            self.handoffs <= limits.handoffs(),
            self.child_directives <= limits.child_directives(),
            self.retained_observations <= limits.retained_observations(),
            self.cancellation_reconciliations <= limits.cancellation_reconciliations(),
        ]
        .into_iter()
        .all(|within_limit| within_limit);
        if bounded {
            Ok(())
        } else {
            Err(limit("orchestrator counter exceeds its independent immutable limit"))
        }
    }

    /// Returns retained candidate revisions.
    #[must_use]
    pub const fn revisions(self) -> u16 {
        self.revisions
    }
    /// Returns started writer cycles.
    #[must_use]
    pub const fn writer_cycles(self) -> u16 {
        self.writer_cycles
    }
    /// Returns started fixer cycles.
    #[must_use]
    pub const fn fixer_cycles(self) -> u16 {
        self.fixer_cycles
    }
    /// Returns started gate cycles.
    #[must_use]
    pub const fn gate_cycles(self) -> u16 {
        self.gate_cycles
    }
    /// Returns started review cycles.
    #[must_use]
    pub const fn review_cycles(self) -> u16 {
        self.review_cycles
    }
    /// Returns retained exact handoffs.
    #[must_use]
    pub const fn handoffs(self) -> u16 {
        self.handoffs
    }
    /// Returns committed directive count.
    #[must_use]
    pub const fn child_directives(self) -> u16 {
        self.child_directives
    }
    /// Returns retained child observations.
    #[must_use]
    pub const fn retained_observations(self) -> u16 {
        self.retained_observations
    }
    /// Returns reconciled cancellation observations.
    #[must_use]
    pub const fn cancellation_reconciliations(self) -> u16 {
        self.cancellation_reconciliations
    }
}

const fn limit(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::LimitExceeded,
        OrchestratorRecoveryAction::NeedsHuman,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::OrchestratorCounters;

    #[test]
    fn genesis_does_not_precount_writer_activation() {
        let counters = OrchestratorCounters::genesis();
        assert_eq!(counters.writer_cycles(), 0);
        assert_eq!(counters.handoffs(), 1);
        assert_eq!(counters.revisions(), 1);
    }
}
