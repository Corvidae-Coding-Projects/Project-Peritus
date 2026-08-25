//! Closed fenced E0 command vocabulary.

use peritus_types::{CommandId, EventId, RevisionTuple, RunId, Sha256Digest};

use crate::{
    AcceptanceCertificate, AgentChildObservation, CandidateBinding, ChildObservation,
    GateChildObservation, Handoff, HandoffActivationObservation, KernelAcceptanceObservation,
    OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction, PendingDirective,
    ReviewChildObservation,
};

mod records;

pub use records::{FixerCompletion, OrchestratorGenesis, ResumeReconciliation};

/// Core semantic payload of one fenced E0 command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchestratorCommandKind {
    /// Creates one exact run and its genesis writer handoff.
    Start {
        /// Boxed complete genesis payload, keeping the closed command representation compact.
        genesis: Box<OrchestratorGenesis>,
    },
    /// Commits or retries one stable effect directive before delivery.
    PublishDirective {
        /// Exact idempotent directive.
        directive: PendingDirective,
    },
    /// Records a matching durable destination acknowledgement.
    AcknowledgeDirective {
        /// Stable acknowledged directive identity.
        directive_id: crate::DirectiveId,
    },
    /// Records exact D3 ownership before role work becomes active.
    ObserveHandoffActivation {
        /// Combined scheduler/collaboration activation truth.
        activation: HandoffActivationObservation,
    },
    /// Records one terminal writer result.
    ObserveWriter {
        /// Exact D0 writer observation.
        observation: AgentChildObservation,
        /// Actual completed writer candidate, absent for terminal non-success.
        candidate: Option<CandidateBinding>,
        /// Same-revision D1/D2/D3 cycle rebound to writer output, absent on non-success.
        quality_cycle: Option<crate::QualityCycleBinding>,
    },
    /// Records one terminal gate result.
    ObserveGates {
        /// Exact D1 gate observation.
        observation: GateChildObservation,
        /// Reviewer handoff required on gate success.
        review_handoff: Option<Handoff>,
    },
    /// Records D2 completion, escalation, or a finding-bearing fixer branch.
    ObserveReview {
        /// Exact current D2 observation.
        observation: ReviewChildObservation,
        /// Fixer handoff required by a needs-fix observation.
        fixer_handoff: Option<Handoff>,
    },
    /// Records one terminal fixer result with complete D2 response coverage.
    ObserveFixer {
        /// Complete checked fixer result.
        completion: FixerCompletion,
    },
    /// Records normal terminal D3 quiescence before B2/B0 evaluation.
    ObserveRoleInfrastructure {
        /// Exact terminal scheduler observation for the current child cycle.
        scheduler: crate::SchedulerChildObservation,
        /// Exact terminal collaboration observation for the current child cycle.
        collaboration: crate::CollaborationChildObservation,
    },
    /// Atomically installs the already checked fixer proposal as current.
    AdvanceCandidate {
        /// Fresh D1/D2/D3 child-cycle binding for the proposed candidate.
        quality_cycle: crate::QualityCycleBinding,
    },
    /// Records an acceptable B2 evaluation certificate without accepting the run.
    RecordAcceptanceCertificate {
        /// Exact B2-derived certificate.
        certificate: AcceptanceCertificate,
    },
    /// Records the durable B0 acceptance outcome.
    ObserveKernelAcceptance {
        /// Exact causally bound B0 event observation.
        observation: KernelAcceptanceObservation,
    },
    /// Stops creation of new work while retaining the resumable phase.
    Pause {
        /// Exact live child heads at the predecessor checkpoint.
        reconciliation: ResumeReconciliation,
    },
    /// Restores the exact paused phase after child-head reconciliation.
    Resume {
        /// Fresh heads required to equal the committed pause checkpoint.
        reconciliation: ResumeReconciliation,
    },
    /// Commits cancellation dominance before child cancellation delivery.
    Cancel {
        /// Stable causal reason digest.
        cause_digest: Sha256Digest,
    },
    /// Reconciles one authoritative child terminal during cancellation.
    ReconcileCancellation {
        /// Exact child terminal observation.
        observation: ChildObservation,
    },
    /// Records authoritative rejection evidence.
    Reject {
        /// Stable rejection evidence digest.
        cause_digest: Sha256Digest,
    },
    /// Records an unrecoverable deterministic failure.
    Fail {
        /// Stable failure evidence digest.
        cause_digest: Sha256Digest,
    },
    /// Records bounded-budget exhaustion.
    Exhaust {
        /// Stable exhaustion evidence digest.
        cause_digest: Sha256Digest,
    },
    /// Computes final cancellation truth; never accepts a caller-selected outcome.
    Finalize,
}

/// One syntax-checked but unprivileged fenced E0 command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorCommand {
    command_id: CommandId,
    event_id: EventId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: OrchestratorCommandKind,
}

impl OrchestratorCommand {
    /// Creates a command with exact genesis/non-genesis predecessor shape.
    ///
    /// # Errors
    /// Rejects an inconsistent sequence/predecessor pair.
    #[allow(clippy::too_many_arguments, reason = "event-sourced fences remain explicit")]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: OrchestratorCommandKind,
    ) -> Result<Self, OrchestratorError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(stale("command predecessor shape is inconsistent"));
        }
        Ok(Self::from_wire(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        ))
    }

    #[allow(clippy::too_many_arguments, reason = "closed-wire fences remain explicit")]
    pub(crate) const fn from_wire(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: OrchestratorCommandKind,
    ) -> Self {
        Self {
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        }
    }

    /// Returns the idempotent command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the exact aggregate run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns expected current sequence, zero at genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns the expected causal predecessor event.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns the complete predecessor state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the exact current revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows the closed semantic payload.
    #[must_use]
    pub const fn kind(&self) -> &OrchestratorCommandKind {
        &self.kind
    }
}

const fn stale(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::StaleState,
        OrchestratorRecoveryAction::Replay,
        detail,
    )
}
