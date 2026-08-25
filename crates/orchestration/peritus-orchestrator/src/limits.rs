//! Independent immutable E0 completion and retention bounds.

use crate::{OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction};

/// Independently bounded production limits for one orchestrator aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrchestratorLimits {
    revisions: u16,
    writer_cycles: u16,
    fixer_cycles: u16,
    gate_cycles: u16,
    review_cycles: u16,
    handoffs: u16,
    child_directives: u16,
    retained_observations: u16,
    artifact_references: u16,
    cancellation_reconciliations: u16,
    event_bytes: u64,
    state_bytes: u64,
}

impl OrchestratorLimits {
    /// Maximum retained revisions.
    pub const MAX_REVISIONS: u16 = 256;
    /// Maximum writer cycles.
    pub const MAX_WRITER_CYCLES: u16 = 256;
    /// Maximum fixer cycles.
    pub const MAX_FIXER_CYCLES: u16 = 256;
    /// Maximum gate cycles.
    pub const MAX_GATE_CYCLES: u16 = 1_024;
    /// Maximum review cycles.
    pub const MAX_REVIEW_CYCLES: u16 = 1_024;
    /// Maximum retained handoffs.
    pub const MAX_HANDOFFS: u16 = 4_096;
    /// Maximum child directives.
    pub const MAX_CHILD_DIRECTIVES: u16 = 4_096;
    /// Maximum retained observations.
    pub const MAX_RETAINED_OBSERVATIONS: u16 = 8_192;
    /// Maximum artifact references per binding or handoff.
    pub const MAX_ARTIFACT_REFERENCES: u16 = 1_024;
    /// Maximum cancellation reconciliation steps.
    pub const MAX_CANCELLATION_RECONCILIATIONS: u16 = 4_096;
    /// Maximum canonical event payload bytes.
    pub const MAX_EVENT_BYTES: u64 = 16 * 1_048_576 - 16;
    /// Maximum canonical checkpoint bytes.
    pub const MAX_STATE_BYTES: u64 = 16 * 1_048_576 - 16;

    /// Creates independently checked nonzero E0 limits.
    ///
    /// # Errors
    /// Rejects a zero or compiled-ceiling-exceeding dimension.
    #[allow(clippy::too_many_arguments, reason = "independent limits remain explicit")]
    pub fn new(
        revisions: u16,
        writer_cycles: u16,
        fixer_cycles: u16,
        gate_cycles: u16,
        review_cycles: u16,
        handoffs: u16,
        child_directives: u16,
        retained_observations: u16,
        artifact_references: u16,
        cancellation_reconciliations: u16,
        event_bytes: u64,
        state_bytes: u64,
    ) -> Result<Self, OrchestratorError> {
        let value = Self::from_wire(
            revisions,
            writer_cycles,
            fixer_cycles,
            gate_cycles,
            review_cycles,
            handoffs,
            child_directives,
            retained_observations,
            artifact_references,
            cancellation_reconciliations,
            event_bytes,
            state_bytes,
        );
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments, reason = "exact closed-wire limit reconstruction")]
    pub(crate) const fn from_wire(
        revisions: u16,
        writer_cycles: u16,
        fixer_cycles: u16,
        gate_cycles: u16,
        review_cycles: u16,
        handoffs: u16,
        child_directives: u16,
        retained_observations: u16,
        artifact_references: u16,
        cancellation_reconciliations: u16,
        event_bytes: u64,
        state_bytes: u64,
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
            artifact_references,
            cancellation_reconciliations,
            event_bytes,
            state_bytes,
        }
    }

    pub(crate) const fn validate(self) -> Result<(), OrchestratorError> {
        let bounded = self.revisions > 0
            && self.revisions <= Self::MAX_REVISIONS
            && self.writer_cycles > 0
            && self.writer_cycles <= Self::MAX_WRITER_CYCLES
            && self.fixer_cycles > 0
            && self.fixer_cycles <= Self::MAX_FIXER_CYCLES
            && self.gate_cycles > 0
            && self.gate_cycles <= Self::MAX_GATE_CYCLES
            && self.review_cycles > 0
            && self.review_cycles <= Self::MAX_REVIEW_CYCLES
            && self.handoffs > 0
            && self.handoffs <= Self::MAX_HANDOFFS
            && self.child_directives > 0
            && self.child_directives <= Self::MAX_CHILD_DIRECTIVES
            && self.retained_observations > 0
            && self.retained_observations <= Self::MAX_RETAINED_OBSERVATIONS
            && self.artifact_references > 0
            && self.artifact_references <= Self::MAX_ARTIFACT_REFERENCES
            && self.cancellation_reconciliations > 0
            && self.cancellation_reconciliations <= Self::MAX_CANCELLATION_RECONCILIATIONS
            && self.event_bytes > 0
            && self.event_bytes <= Self::MAX_EVENT_BYTES
            && self.state_bytes > 0
            && self.state_bytes <= Self::MAX_STATE_BYTES;
        if bounded {
            Ok(())
        } else {
            Err(OrchestratorError::new(
                OrchestratorErrorKind::LimitExceeded,
                OrchestratorRecoveryAction::CorrectInput,
                "orchestrator limit is zero or exceeds its compiled ceiling",
            ))
        }
    }

    /// Returns the total retained revision bound.
    #[must_use]
    pub const fn revisions(self) -> u16 {
        self.revisions
    }
    /// Returns the writer-cycle bound.
    #[must_use]
    pub const fn writer_cycles(self) -> u16 {
        self.writer_cycles
    }
    /// Returns the fixer-cycle bound.
    #[must_use]
    pub const fn fixer_cycles(self) -> u16 {
        self.fixer_cycles
    }
    /// Returns the gate-cycle bound.
    #[must_use]
    pub const fn gate_cycles(self) -> u16 {
        self.gate_cycles
    }
    /// Returns the review-cycle bound.
    #[must_use]
    pub const fn review_cycles(self) -> u16 {
        self.review_cycles
    }
    /// Returns the handoff bound.
    #[must_use]
    pub const fn handoffs(self) -> u16 {
        self.handoffs
    }
    /// Returns the child-directive bound.
    #[must_use]
    pub const fn child_directives(self) -> u16 {
        self.child_directives
    }
    /// Returns the retained-observation bound.
    #[must_use]
    pub const fn retained_observations(self) -> u16 {
        self.retained_observations
    }
    /// Returns the artifact-reference bound.
    #[must_use]
    pub const fn artifact_references(self) -> u16 {
        self.artifact_references
    }
    /// Returns the cancellation-reconciliation bound.
    #[must_use]
    pub const fn cancellation_reconciliations(self) -> u16 {
        self.cancellation_reconciliations
    }
    /// Returns the event byte bound.
    #[must_use]
    pub const fn event_bytes(self) -> u64 {
        self.event_bytes
    }
    /// Returns the checkpoint byte bound.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.state_bytes
    }
}
