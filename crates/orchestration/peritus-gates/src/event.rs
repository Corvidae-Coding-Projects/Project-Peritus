//! Immutable D1 events and reducer transitions.

use peritus_types::{
    CommandId, EventId, EventSequence, GateExecutionId, GateId, RevisionTuple, RunId, Sha256Digest,
};

use crate::{
    ActiveAttempt, GateAttemptResult, GateEvidenceReceipt, GateResumePhase, GateRunState,
    RecoveryDisposition,
};

/// Closed semantic fact accepted by the D1 reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GateEventKind {
    /// The exact revision/snapshot run started.
    RunStarted {
        /// Complete clean C1 snapshot binding digest.
        snapshot_digest: Sha256Digest,
    },
    /// A fresh attempt was persisted before dispatch.
    AttemptPrepared {
        /// Planned gate identity.
        gate_id: GateId,
        /// Complete fresh attempt binding.
        attempt: ActiveAttempt,
    },
    /// C4 dispatch was accepted.
    AttemptDispatched {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact active execution identity.
        execution_id: GateExecutionId,
    },
    /// A strict C4 terminal was observed.
    ResultObserved {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact active execution identity.
        execution_id: GateExecutionId,
        /// Strict normalized terminal.
        result: GateAttemptResult,
    },
    /// Recovery classified prior effect ownership.
    RecoveryClassified {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact reconciled execution identity.
        execution_id: GateExecutionId,
        /// Closed reconciliation observation.
        disposition: RecoveryDisposition,
    },
    /// Required exact-revision evidence was admitted.
    EvidencePublished {
        /// Planned gate identity.
        gate_id: GateId,
        /// Exact passing execution identity.
        execution_id: GateExecutionId,
        /// Complete admitted evidence receipt.
        receipt: GateEvidenceReceipt,
    },
    /// Run cancellation began.
    CancellationStarted,
    /// Progress was suspended from an exact nonterminal phase.
    RunPaused {
        /// Phase that a resume must restore.
        resume_phase: GateResumePhase,
    },
    /// The exact phase retained by the pause was restored.
    RunResumed {
        /// Phase restored by this event.
        resume_phase: GateResumePhase,
    },
    /// Deterministic terminal aggregation was committed.
    RunFinalized,
}

/// One canonical event carrying all predecessor and successor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateEvent {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: GateEventKind,
}

impl GateEvent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: GateEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            sequence,
            previous_event,
            run_id,
            revision,
            prior_state_digest,
            successor_state_digest,
            kind,
        }
    }

    /// Returns the event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Returns the causative command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the one-based aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns the exact causal predecessor.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Returns the run aggregate identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the exact revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the predecessor state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the successor state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Borrows the closed semantic fact.
    #[must_use]
    pub const fn kind(&self) -> &GateEventKind {
        &self.kind
    }
}

/// Pure accepted event plus complete successor state retained until C0 commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateTransition {
    event: GateEvent,
    state: GateRunState,
}

impl GateTransition {
    pub(crate) const fn new(event: GateEvent, state: GateRunState) -> Self {
        Self { event, state }
    }
    /// Borrows the immutable event.
    #[must_use]
    pub const fn event(&self) -> &GateEvent {
        &self.event
    }
    /// Borrows the complete successor state.
    #[must_use]
    pub const fn state(&self) -> &GateRunState {
        &self.state
    }
    /// Consumes the transition after a C0 commit observation.
    #[must_use]
    pub fn into_state(self) -> GateRunState {
        self.state
    }
}
