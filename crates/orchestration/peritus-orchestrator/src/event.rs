//! Immutable E0 facts and pure successor transitions.

use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, RunId, Sha256Digest};

use crate::command::OrchestratorGenesis;
use crate::{
    AcceptanceCertificate, AgentChildObservation, CandidateBinding, ChildObservation,
    FixerCompletion, GateChildObservation, Handoff, HandoffActivationObservation,
    KernelAcceptanceObservation, OrchestratorPhase, OrchestratorState, OrchestratorTerminal,
    PendingDirective, ResumeReconciliation, ReviewChildObservation,
};

/// Closed semantic fact accepted by the E0 reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchestratorEventKind {
    /// The exact orchestrator run and writer handoff started.
    Started {
        /// Boxed complete genesis payload, keeping the closed event representation compact.
        genesis: Box<OrchestratorGenesis>,
    },
    /// A stable directive was committed before external publication.
    DirectivePublished {
        /// Exact idempotent directive as requested.
        directive: PendingDirective,
    },
    /// The destination durably acknowledged the exact directive.
    DirectiveAcknowledged {
        /// Stable acknowledged directive identity.
        directive_id: crate::DirectiveId,
    },
    /// D3 durably activated exact handoff ownership.
    HandoffActivated {
        /// Combined scheduler/collaboration activation truth.
        activation: HandoffActivationObservation,
    },
    /// A terminal writer result was observed.
    WriterObserved {
        /// Exact D0 writer observation.
        observation: AgentChildObservation,
        /// Actual completed writer candidate installed before D1, absent on failure.
        candidate: Option<CandidateBinding>,
        /// Same-revision quality cycle installed with the writer candidate.
        quality_cycle: Option<crate::QualityCycleBinding>,
    },
    /// A terminal gate result was observed.
    GatesObserved {
        /// Exact D1 gate observation.
        observation: GateChildObservation,
        /// Reviewer handoff admitted on pass.
        review_handoff: Option<Handoff>,
    },
    /// A D2 result or needs-fix branch was observed.
    ReviewObserved {
        /// Exact D2 review observation.
        observation: ReviewChildObservation,
        /// Fixer handoff admitted on needs-fix.
        fixer_handoff: Option<Handoff>,
    },
    /// A terminal fixer result and complete response coverage were observed.
    FixerObserved {
        /// Complete checked fixer result.
        completion: FixerCompletion,
    },
    /// Current-cycle D3 scheduler and collaboration aggregates became terminal.
    RoleInfrastructureObserved {
        /// Exact terminal scheduler observation.
        scheduler: crate::SchedulerChildObservation,
        /// Exact terminal collaboration observation.
        collaboration: crate::CollaborationChildObservation,
    },
    /// The checked fixer proposal became the exact current candidate.
    CandidateAdvanced {
        /// Complete successor candidate binding.
        candidate: CandidateBinding,
        /// Fresh child-cycle binding installed atomically with the candidate.
        quality_cycle: crate::QualityCycleBinding,
    },
    /// An acceptable B2 evaluation certificate was recorded.
    AcceptanceCertificateRecorded {
        /// Exact B2-derived certificate.
        certificate: AcceptanceCertificate,
    },
    /// Durable B0 acceptance truth was observed.
    KernelAcceptanceObserved {
        /// Exact causally bound B0 observation.
        observation: KernelAcceptanceObservation,
    },
    /// New work stopped while the exact resumable phase was retained.
    Paused {
        /// Complete paused phase.
        phase: OrchestratorPhase,
        /// Exact child heads committed with the pause.
        reconciliation: ResumeReconciliation,
    },
    /// The exact paused phase was restored.
    Resumed {
        /// Complete restored phase.
        phase: OrchestratorPhase,
        /// Fresh heads equal to the committed pause checkpoint.
        reconciliation: ResumeReconciliation,
    },
    /// Cancellation became dominant before external cancellation effects.
    CancellationRequested {
        /// Stable cancellation cause digest.
        cause_digest: Sha256Digest,
    },
    /// One authoritative child terminal was reconciled under cancellation.
    CancellationReconciled {
        /// Exact child observation.
        observation: ChildObservation,
    },
    /// An authoritative rejection was committed.
    Rejected {
        /// Truthful immutable terminal fact.
        terminal: OrchestratorTerminal,
    },
    /// An unrecoverable deterministic failure was committed.
    Failed {
        /// Truthful immutable terminal fact.
        terminal: OrchestratorTerminal,
    },
    /// Bounded exhaustion was committed.
    Exhausted {
        /// Truthful immutable terminal fact.
        terminal: OrchestratorTerminal,
    },
    /// Final cancellation truth was computed from quiescent owned children.
    Finalized {
        /// Truthful immutable terminal fact.
        terminal: OrchestratorTerminal,
    },
}

/// One canonical event carrying exact predecessor and successor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorEvent {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: OrchestratorEventKind,
}

impl OrchestratorEvent {
    #[allow(clippy::too_many_arguments, reason = "closed-wire event fences remain explicit")]
    pub(crate) const fn from_wire(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: OrchestratorEventKind,
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
    /// Returns the exact aggregate run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the command's exact current-revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Borrows the closed accepted semantic fact.
    #[must_use]
    pub const fn kind(&self) -> &OrchestratorEventKind {
        &self.kind
    }
}

/// Pure accepted event plus complete successor state retained for atomic commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorTransition {
    event: OrchestratorEvent,
    state: OrchestratorState,
}

impl OrchestratorTransition {
    pub(crate) const fn new(event: OrchestratorEvent, state: OrchestratorState) -> Self {
        Self { event, state }
    }

    /// Borrows the immutable event.
    #[must_use]
    pub const fn event(&self) -> &OrchestratorEvent {
        &self.event
    }
    /// Borrows the complete successor state.
    #[must_use]
    pub const fn state(&self) -> &OrchestratorState {
        &self.state
    }
    /// Consumes the transition after durable commit.
    #[must_use]
    pub fn into_state(self) -> OrchestratorState {
        self.state
    }
}
