//! Closed pure D1 command vocabulary.

use peritus_types::{
    CommandId, EventId, GateExecutionId, GateId, RevisionTuple, RunId, Sha256Digest,
};

use crate::{ActiveAttempt, GateAttemptResult, GateEvidenceReceipt};

/// Checked recovery observation for one dispatched attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryDisposition {
    /// C2/C4 established prior effect terminality and a fresh action is safe.
    SafeToRetry,
    /// Recovery established a non-retryable terminal failure.
    TerminalFailure,
    /// The prior effect is still active; no retry may begin.
    StillActive,
}

/// Core semantic payload of one D1 reducer command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GateCommandKind {
    /// Starts the run against one clean immutable snapshot binding.
    StartRun {
        /// Complete C1 snapshot binding digest.
        snapshot_digest: Sha256Digest,
    },
    /// Persists a fresh exact attempt before any effect dispatch.
    PrepareAttempt {
        /// Planned gate identity.
        gate_id: GateId,
        /// Complete fresh attempt binding.
        attempt: ActiveAttempt,
    },
    /// Records that C4 accepted dispatch and may own an effect.
    MarkDispatched {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact active execution.
        execution_id: GateExecutionId,
    },
    /// Records one strict typed C4 terminal.
    ObserveResult {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact active execution.
        execution_id: GateExecutionId,
        /// Complete normalized result.
        result: GateAttemptResult,
    },
    /// Records recovery of an indeterminate/owned prior effect.
    ClassifyRecovery {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact prior execution.
        execution_id: GateExecutionId,
        /// Recovery observation.
        disposition: RecoveryDisposition,
    },
    /// Records exact admitted C0 evidence for a passing result.
    PublishEvidence {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact passing execution.
        execution_id: GateExecutionId,
        /// Complete publication receipt.
        receipt: GateEvidenceReceipt,
    },
    /// Begins idempotent run cancellation.
    BeginCancellation,
    /// Commits the only deterministic terminal aggregation.
    FinalizeRun,
}

/// One syntax-checked but unprivileged reducer command with predecessor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateCommand {
    command_id: CommandId,
    event_id: EventId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: GateCommandKind,
}

impl GateCommand {
    /// Creates a command with exact genesis/non-genesis predecessor shape.
    ///
    /// # Errors
    /// Rejects inconsistent zero-sequence/predecessor combinations.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: GateCommandKind,
    ) -> Result<Self, crate::GateError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(crate::error::reject(
                crate::GateRejection::ReplayMismatch,
                "command predecessor shape is inconsistent",
            ));
        }
        Ok(Self {
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        })
    }

    /// Returns the idempotent C0 command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the exact run aggregate identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the expected current aggregate sequence, zero at genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns the exact expected prior event.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns the expected current state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the exact revision binding.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows the closed semantic command.
    #[must_use]
    pub const fn kind(&self) -> &GateCommandKind {
        &self.kind
    }
}
