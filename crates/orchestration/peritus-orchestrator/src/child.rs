//! Checked authority-free observations of E0 child aggregate truth.

mod agent;
mod d3;
/// D1 gate-run observations and their canonical wire representation.
pub mod gates;
mod kernel;
mod review;

use peritus_types::{EventId, EventSequence, RevisionTuple, RunId, Sha256Digest};

use crate::{OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction};

pub use agent::{AgentChildObservation, FixerResponseIdentity};
pub use d3::{
    CollaborationChildObservation, HandoffActivationObservation, SchedulerChildObservation,
};
pub use gates::{GateChildObservation, GateObservationClass};
pub use kernel::{KernelAcceptanceObservation, KernelAcceptanceOutcome};
pub use review::{
    ReviewChildObservation, ReviewFixerObservation, ReviewFixerRecord, ReviewObservationClass,
};

/// Closed child aggregate boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ChildAggregateKind {
    /// D0 agent turn.
    Agent,
    /// D1 gate run.
    Gates,
    /// D2 review run.
    Review,
    /// D3 scheduler.
    Scheduler,
    /// D3 collaboration graph.
    Collaboration,
    /// B0 lifecycle kernel.
    Kernel,
}

/// Normalized child terminal truth retained by E0.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ChildTerminalClass {
    /// The child completed its own bounded responsibility.
    Completed,
    /// The child reported a deterministic failure.
    Failed,
    /// Cancellation completed without success.
    Cancelled,
    /// External truth could not be established.
    Indeterminate,
    /// The child requires human intervention.
    NeedsHuman,
    /// B0 requires another candidate cycle.
    NeedsChanges,
    /// B0 durably accepted the exact revision.
    Accepted,
}

/// Closed non-success settlement truth for an owned child that cannot yield a terminal record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CancellationClassificationKind {
    /// The exact owned child is durably known to be unreachable.
    Unreachable,
    /// Available projection truth cannot identify one authoritative terminal.
    Ambiguous,
}

/// Durable evidence-backed cancellation settlement that can never represent success.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CancellationChildClassification {
    aggregate: ChildAggregateKind,
    revision: RevisionTuple,
    kind: CancellationClassificationKind,
    evidence_digest: Sha256Digest,
}

impl CancellationChildClassification {
    /// Records evidence that an exact child is unreachable during cancellation.
    ///
    /// # Errors
    ///
    /// Returns an error when the evidence digest is zero.
    pub fn unreachable(
        aggregate: ChildAggregateKind,
        revision: RevisionTuple,
        evidence_digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        Self::new(aggregate, revision, CancellationClassificationKind::Unreachable, evidence_digest)
    }

    /// Records evidence that an exact child terminal is irreconcilably ambiguous.
    ///
    /// # Errors
    ///
    /// Returns an error when the evidence digest is zero.
    pub fn ambiguous(
        aggregate: ChildAggregateKind,
        revision: RevisionTuple,
        evidence_digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        Self::new(aggregate, revision, CancellationClassificationKind::Ambiguous, evidence_digest)
    }

    fn new(
        aggregate: ChildAggregateKind,
        revision: RevisionTuple,
        kind: CancellationClassificationKind,
        evidence_digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        if evidence_digest.as_bytes().iter().all(|byte| *byte == 0) {
            return Err(binding("cancellation classification evidence must be nonzero"));
        }
        Ok(Self { aggregate, revision, kind, evidence_digest })
    }

    pub(crate) fn from_wire(
        aggregate: ChildAggregateKind,
        revision: RevisionTuple,
        kind: CancellationClassificationKind,
        evidence_digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        Self::new(aggregate, revision, kind, evidence_digest)
    }

    #[must_use]
    /// Returns the owned child aggregate being settled.
    pub const fn aggregate(self) -> ChildAggregateKind {
        self.aggregate
    }
    #[must_use]
    /// Returns the candidate revision at which settlement was classified.
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns whether the child was unreachable or its terminal was ambiguous.
    pub const fn kind(self) -> CancellationClassificationKind {
        self.kind
    }
    #[must_use]
    /// Returns the nonzero evidence digest supporting the classification.
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}

/// Exact authoritative head retained with an observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChildHead {
    aggregate: ChildAggregateKind,
    sequence: EventSequence,
    last_event_id: EventId,
    state_digest: Sha256Digest,
    terminal: Option<ChildTerminalClass>,
}

impl ChildHead {
    /// Records a nonterminal head returned by a trusted child projection port.
    ///
    /// This value is suitable for pause/resume reconciliation only; authoritative
    /// completion still requires one of the aggregate-specific checked adapters.
    ///
    /// # Errors
    ///
    /// Returns an error when the state digest is zero.
    pub fn observed(
        aggregate: ChildAggregateKind,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        Self::new(aggregate, sequence, last_event_id, state_digest, None)
    }

    pub(crate) fn new(
        aggregate: ChildAggregateKind,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        terminal: Option<ChildTerminalClass>,
    ) -> Result<Self, OrchestratorError> {
        if state_digest.as_bytes().iter().all(|byte| *byte == 0) {
            return Err(binding("child state digest must be nonzero"));
        }
        Ok(Self { aggregate, sequence, last_event_id, state_digest, terminal })
    }

    #[must_use]
    /// Returns the aggregate represented by this head.
    pub const fn aggregate(self) -> ChildAggregateKind {
        self.aggregate
    }
    #[must_use]
    /// Returns the aggregate event sequence at this head.
    pub const fn sequence(self) -> EventSequence {
        self.sequence
    }
    #[must_use]
    /// Returns the aggregate event identity at this head.
    pub const fn last_event_id(self) -> EventId {
        self.last_event_id
    }
    #[must_use]
    /// Returns the authoritative child state digest at this head.
    pub const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }
    #[must_use]
    /// Returns normalized terminal truth, or `None` for an active head.
    pub const fn terminal(self) -> Option<ChildTerminalClass> {
        self.terminal
    }

    /// Returns whether this is a nonterminal head suitable for checkpoint reconciliation.
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.terminal.is_none()
    }
}

/// Closed union of checked child observations admitted by E0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildObservation {
    /// A terminal D0 writer or fixer observation.
    Agent(AgentChildObservation),
    /// A terminal D1 gate-run observation.
    Gates(GateChildObservation),
    /// A D2 needs-fix or terminal review observation.
    Review(ReviewChildObservation),
    /// A D2 head proving fixer responses were durably recorded.
    ReviewFixer(ReviewFixerObservation),
    /// A terminal D3 scheduler observation.
    Scheduler(SchedulerChildObservation),
    /// A terminal D3 collaboration observation.
    Collaboration(CollaborationChildObservation),
    /// A combined D3 scheduler/collaboration activation observation.
    HandoffActivation(HandoffActivationObservation),
    /// A durable B0 acceptance-lifecycle observation.
    KernelAcceptance(KernelAcceptanceObservation),
    /// Evidence-backed non-success settlement for unreachable or ambiguous cancellation truth.
    CancellationClassification(CancellationChildClassification),
}

impl ChildObservation {
    /// Returns the child aggregate represented by the observation.
    #[must_use]
    pub const fn aggregate(&self) -> ChildAggregateKind {
        match self {
            Self::Agent(_) => ChildAggregateKind::Agent,
            Self::Gates(_) => ChildAggregateKind::Gates,
            Self::Review(_) | Self::ReviewFixer(_) => ChildAggregateKind::Review,
            Self::Scheduler(_) => ChildAggregateKind::Scheduler,
            Self::Collaboration(_) | Self::HandoffActivation(_) => {
                ChildAggregateKind::Collaboration
            }
            Self::KernelAcceptance(_) => ChildAggregateKind::Kernel,
            Self::CancellationClassification(value) => value.aggregate(),
        }
    }

    /// Returns the retained child head when this observation has one.
    #[must_use]
    pub const fn head(&self) -> Option<ChildHead> {
        match self {
            Self::Agent(value) => Some(value.head()),
            Self::Gates(value) => Some(value.head()),
            Self::Review(value) => Some(value.head()),
            Self::ReviewFixer(value) => Some(value.head()),
            Self::Scheduler(value) => Some(value.head()),
            Self::Collaboration(value) => Some(value.head()),
            Self::HandoffActivation(_)
            | Self::KernelAcceptance(_)
            | Self::CancellationClassification(_) => None,
        }
    }

    /// Returns the candidate revision represented by the observation.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        match self {
            Self::Agent(value) => value.revision(),
            Self::Gates(value) => value.revision(),
            Self::Review(value) => value.revision(),
            Self::ReviewFixer(value) => value.revision(),
            Self::Scheduler(value) => value.revision(),
            Self::Collaboration(value) => value.revision(),
            Self::HandoffActivation(value) => value.revision(),
            Self::KernelAcceptance(value) => value.revision(),
            Self::CancellationClassification(value) => value.revision(),
        }
    }

    /// Returns the overall E0 run identity when the child observation carries it.
    #[must_use]
    pub const fn run_id(&self) -> Option<RunId> {
        match self {
            Self::Agent(value) => Some(value.run_id()),
            Self::Gates(value) => Some(value.run_id()),
            Self::Review(value) => Some(value.run_id()),
            Self::ReviewFixer(value) => Some(value.run_id()),
            Self::Scheduler(_)
            | Self::Collaboration(_)
            | Self::HandoffActivation(_)
            | Self::CancellationClassification(_) => None,
            Self::KernelAcceptance(value) => Some(value.run_id()),
        }
    }
}

const fn binding(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::BindingMismatch,
        OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}

const fn stale(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::StaleState,
        OrchestratorRecoveryAction::Replay,
        detail,
    )
}
