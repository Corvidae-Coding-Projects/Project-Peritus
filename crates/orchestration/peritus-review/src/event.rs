//! Immutable D2 facts and pure successor transitions.

use peritus_evidence::EvidenceId;
use peritus_types::{
    CommandId, EventId, EventSequence, FindingId, ReviewCycleId, RevisionTuple, RunId, Sha256Digest,
};

use crate::{
    FixerResponse, ObservedWaiver, ReviewAssignment, ReviewBinding, ReviewLimits, ReviewRunState,
    ReviewSubmission,
};

/// Closed semantic fact accepted by the D2 reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewEventKind {
    /// The exact review run started.
    RunStarted {
        /// Immutable contract, candidate, revision, and producer binding.
        binding: ReviewBinding,
        /// Bounds fixed for the complete run.
        limits: ReviewLimits,
    },
    /// The candidate/revision binding advanced and prior evidence became historical.
    RevisionAdvanced {
        /// Complete checked successor binding.
        binding: ReviewBinding,
    },
    /// One reviewer cycle was assigned.
    ReviewerAssigned {
        /// Complete checked reviewer assignment.
        assignment: ReviewAssignment,
    },
    /// One complete structured review was accepted.
    ReviewSubmitted {
        /// Complete checked structured submission.
        submission: ReviewSubmission,
    },
    /// Duplicate findings were reconciled under an existing canonical identity.
    DuplicatesReconciled {
        /// Existing finding that retains the merged provenance.
        canonical: FindingId,
        /// Canonical nonempty identities absorbed by the canonical finding.
        duplicates: Vec<FindingId>,
        /// Digest binding the explicit reconciliation decision.
        reconciliation_digest: Sha256Digest,
    },
    /// Fixer evidence was recorded without implicit closure.
    FixerResponseRecorded {
        /// Current finding receiving the response.
        finding_id: FindingId,
        /// Checked fixer evidence and proposed outcome.
        response: FixerResponse,
    },
    /// A reviewer confirmed resolution.
    ResolutionConfirmed {
        /// Current finding being resolved.
        finding_id: FindingId,
        /// Assigned independent reviewer cycle providing confirmation.
        reviewer_cycle: ReviewCycleId,
        /// Exact pending fixer response being confirmed.
        pending_response_digest: Sha256Digest,
        /// Bounded confirmation evidence.
        evidence: Vec<EvidenceId>,
        /// Digest binding the reviewer confirmation.
        confirmation_digest: Sha256Digest,
    },
    /// A reviewer confirmed invalidation.
    InvalidationConfirmed {
        /// Current finding being invalidated.
        finding_id: FindingId,
        /// Assigned independent reviewer cycle providing confirmation.
        reviewer_cycle: ReviewCycleId,
        /// Exact pending dispute being confirmed.
        pending_response_digest: Sha256Digest,
        /// Bounded confirmation evidence.
        evidence: Vec<EvidenceId>,
        /// Digest binding the reviewer confirmation.
        confirmation_digest: Sha256Digest,
    },
    /// A reviewer confirmed provenance-preserving supersession.
    SupersessionConfirmed {
        /// Current finding being superseded.
        finding_id: FindingId,
        /// Current finding that supersedes it.
        superseding: FindingId,
        /// Assigned independent reviewer cycle providing confirmation.
        reviewer_cycle: ReviewCycleId,
        /// Exact pending supersession response being confirmed.
        pending_response_digest: Sha256Digest,
        /// Bounded confirmation evidence.
        evidence: Vec<EvidenceId>,
        /// Digest binding the reviewer confirmation.
        confirmation_digest: Sha256Digest,
    },
    /// An existing external waiver request was recorded.
    WaiverRequested {
        /// Current finding for which authority was requested.
        finding_id: FindingId,
        /// Checked fixer-authored waiver request.
        request: FixerResponse,
    },
    /// An exact external B2 waiver observation was consumed.
    WaiverObserved {
        /// Already-authorized external waiver observation and request binding.
        waiver: ObservedWaiver,
    },
    /// One unsubmitted cycle was cancelled.
    CycleCancelled {
        /// Assigned cycle that was cancelled.
        cycle_id: ReviewCycleId,
    },
    /// The run was cancelled without success.
    RunCancelled,
    /// Active review progress was durably suspended.
    RunPaused,
    /// The exact active review phase was restored.
    RunResumed,
    /// Explicit budget exhaustion was retained as escalation evidence.
    BudgetExhausted {
        /// Digest of the external bounded budget-exhaustion reason.
        reason_digest: Sha256Digest,
    },
    /// An explicitly unrecoverable failure was retained.
    RunFailed {
        /// Digest of the external bounded failure observation.
        failure_digest: Sha256Digest,
    },
    /// Deterministic terminal review completion was committed.
    RunFinalized,
}

/// One canonical event carrying predecessor and successor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewEvent {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: ReviewEventKind,
}

impl ReviewEvent {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: ReviewEventKind,
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
    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the command's exact current-revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Borrows the closed accepted semantic fact.
    #[must_use]
    pub const fn kind(&self) -> &ReviewEventKind {
        &self.kind
    }
}

/// Pure accepted event plus complete successor state retained for C0 commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewTransition {
    event: ReviewEvent,
    state: ReviewRunState,
}

impl ReviewTransition {
    pub(super) const fn new(event: ReviewEvent, state: ReviewRunState) -> Self {
        Self { event, state }
    }
    /// Borrows the immutable event.
    #[must_use]
    pub const fn event(&self) -> &ReviewEvent {
        &self.event
    }
    /// Borrows the complete successor state.
    #[must_use]
    pub const fn state(&self) -> &ReviewRunState {
        &self.state
    }
    /// Consumes the transition after durable commit.
    #[must_use]
    pub fn into_state(self) -> ReviewRunState {
        self.state
    }
}
