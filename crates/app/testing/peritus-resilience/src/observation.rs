//! Authoritative state and staged subject observations.

use std::error::Error;
use std::fmt;

use crate::config::HARD_MAX_MILESTONES;
use crate::evidence_observation::HARD_MAX_EVIDENCE_ANCHORS;
use crate::recovery_state::{RecoveredStateObservation, RecoveryAccounting};
use crate::{
    CorruptTarget, EvidenceAnchor, EvidenceDigest, EvidenceId, FaultInjection, Milestone,
    OwnershipObservation, RecoveryOutcome, ResourceUsage, RetryUsage, ScenarioId,
};

/// Observation collection validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationError {
    /// Too many milestones were supplied.
    TooManyMilestones {
        /// Number of supplied milestones.
        actual: usize,
        /// Hard milestone-count ceiling.
        maximum: usize,
    },
    /// Milestone sequence numbers were not strictly increasing.
    MilestoneOrder,
    /// Too many evidence anchors were supplied.
    TooManyEvidenceAnchors {
        /// Number of supplied evidence anchors.
        actual: usize,
        /// Hard evidence-anchor-count ceiling.
        maximum: usize,
    },
    /// Evidence anchors reused an identifier.
    DuplicateEvidenceId(EvidenceId),
}

impl fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyMilestones { actual, maximum } => {
                write!(formatter, "{actual} milestones exceed maximum {maximum}")
            }
            Self::MilestoneOrder => formatter.write_str("milestones must be strictly increasing"),
            Self::TooManyEvidenceAnchors { actual, maximum } => {
                write!(formatter, "{actual} evidence anchors exceed maximum {maximum}")
            }
            Self::DuplicateEvidenceId(id) => {
                write!(formatter, "duplicate evidence identifier: {id}")
            }
        }
    }
}

impl Error for ObservationError {}

/// Authoritative lifecycle observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalState {
    /// Governed work may continue.
    Active,
    /// Work is durably paused.
    Paused,
    /// Work ended blocked on external authority or availability.
    Blocked,
    /// Work ended with an explicit failure.
    Failed,
    /// Work ended by cancellation.
    Cancelled,
    /// A governed budget was exhausted.
    Exhausted,
    /// Current exact evidence authorized acceptance.
    Accepted,
}

/// Acceptance-specific facts kept distinct from terminal prose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptanceObservation {
    terminal: TerminalState,
    revision_bound: bool,
    evidence_current: bool,
}

impl AcceptanceObservation {
    /// Creates a direct acceptance observation, including contradictory values for negative tests.
    #[must_use]
    pub const fn new(
        terminal: TerminalState,
        revision_bound: bool,
        evidence_current: bool,
    ) -> Self {
        Self { terminal, revision_bound, evidence_current }
    }
    /// Returns the authoritative terminal state.
    #[must_use]
    pub const fn terminal(self) -> TerminalState {
        self.terminal
    }
    /// Returns whether acceptance is bound to the recovered exact revision.
    #[must_use]
    pub const fn revision_bound(self) -> bool {
        self.revision_bound
    }
    /// Returns whether all acceptance evidence is current and complete.
    #[must_use]
    pub const fn evidence_current(self) -> bool {
        self.evidence_current
    }
}

/// Journal integrity after recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalHealth {
    /// Hash chain and sequence remained valid.
    Verified,
    /// Recovery replayed and then verified the hash chain and sequence.
    RecoveredAndVerified,
    /// Injected hash divergence was detected and mutation stopped.
    HashDivergenceDetected,
    /// Journal could not be verified or diagnosed.
    Unavailable,
}

/// Referenced content-addressed object state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactHealth {
    /// Every referenced object digest was verified.
    Verified,
    /// Exact injected object divergence was detected and contained.
    DivergenceDetected,
    /// Referenced object state could not be verified.
    Unavailable,
}

/// Projection state after recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionHealth {
    /// Projection agreed with deterministic journal replay.
    Verified,
    /// Corrupt projection was discarded, rebuilt, and verified.
    RebuiltAndVerified,
    /// Projection remained corrupt or divergent.
    Divergent,
    /// Projection was not inspected.
    Unavailable,
}

/// Exact corruption detection and post-recovery admission observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CorruptionObservation {
    detected: Option<CorruptTarget>,
    mutation_admitted: bool,
}

impl CorruptionObservation {
    /// Creates a direct corruption observation.
    #[must_use]
    pub const fn new(detected: Option<CorruptTarget>, mutation_admitted: bool) -> Self {
        Self { detected, mutation_admitted }
    }
    /// Returns the exact target diagnosed as corrupt.
    #[must_use]
    pub const fn detected(self) -> Option<CorruptTarget> {
        self.detected
    }
    /// Returns whether mutation was admitted after recovery/diagnosis.
    #[must_use]
    pub const fn mutation_admitted(self) -> bool {
        self.mutation_admitted
    }
}

/// Direct preparation-stage observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationObservation {
    scenario_id: ScenarioId,
    terminal: TerminalState,
    journal_head: EvidenceDigest,
}

impl PreparationObservation {
    /// Creates a prepared-baseline observation.
    #[must_use]
    pub const fn new(
        scenario_id: ScenarioId,
        terminal: TerminalState,
        journal_head: EvidenceDigest,
    ) -> Self {
        Self { scenario_id, terminal, journal_head }
    }
    /// Returns the scenario identity.
    #[must_use]
    pub const fn scenario_id(&self) -> &ScenarioId {
        &self.scenario_id
    }
    /// Returns the baseline terminal state.
    #[must_use]
    pub const fn terminal(&self) -> TerminalState {
        self.terminal
    }
    /// Returns the exact baseline journal-head digest.
    #[must_use]
    pub const fn journal_head(&self) -> EvidenceDigest {
        self.journal_head
    }
}

/// Direct injection-stage observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisruptionObservation {
    scenario_id: ScenarioId,
    fault: FaultInjection,
    reached: bool,
}

impl DisruptionObservation {
    /// Creates a fault-boundary observation.
    #[must_use]
    pub const fn new(scenario_id: ScenarioId, fault: FaultInjection, reached: bool) -> Self {
        Self { scenario_id, fault, reached }
    }
    /// Returns the scenario identity.
    #[must_use]
    pub const fn scenario_id(&self) -> &ScenarioId {
        &self.scenario_id
    }
    /// Returns the fault the subject actually armed.
    #[must_use]
    pub const fn fault(&self) -> FaultInjection {
        self.fault
    }
    /// Returns whether execution reached and triggered the exact fault.
    #[must_use]
    pub const fn reached(&self) -> bool {
        self.reached
    }
}

/// Complete bounded post-recovery observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryObservation {
    scenario_id: ScenarioId,
    outcome: RecoveryOutcome,
    acceptance: AcceptanceObservation,
    journal: JournalHealth,
    artifacts: ArtifactHealth,
    projection: ProjectionHealth,
    corruption: CorruptionObservation,
    ownership: OwnershipObservation,
    retries: RetryUsage,
    resources: ResourceUsage,
    temporary_objects: u16,
    evidence: Vec<EvidenceAnchor>,
    milestones: Vec<Milestone>,
}

impl RecoveryObservation {
    /// Creates a bounded recovery observation and canonicalizes evidence-anchor ordering.
    ///
    /// # Errors
    ///
    /// Returns [`ObservationError`] when evidence or milestones exceed their hard bounds, an
    /// evidence identifier is duplicated, or milestone sequence numbers are not strictly
    /// increasing.
    pub fn new(
        scenario_id: ScenarioId,
        outcome: RecoveryOutcome,
        state: RecoveredStateObservation,
        accounting: RecoveryAccounting,
        mut evidence: Vec<EvidenceAnchor>,
        milestones: Vec<Milestone>,
    ) -> Result<Self, ObservationError> {
        if evidence.len() > HARD_MAX_EVIDENCE_ANCHORS {
            return Err(ObservationError::TooManyEvidenceAnchors {
                actual: evidence.len(),
                maximum: HARD_MAX_EVIDENCE_ANCHORS,
            });
        }
        evidence.sort_by(|left, right| {
            left.kind().cmp(&right.kind()).then_with(|| left.id().cmp(right.id()))
        });
        if let Some(pair) = evidence.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
            return Err(ObservationError::DuplicateEvidenceId(pair[0].id().clone()));
        }
        let maximum = usize::from(HARD_MAX_MILESTONES);
        if milestones.len() > maximum {
            return Err(ObservationError::TooManyMilestones { actual: milestones.len(), maximum });
        }
        if milestones.windows(2).any(|pair| pair[0].sequence() >= pair[1].sequence()) {
            return Err(ObservationError::MilestoneOrder);
        }
        Ok(Self {
            scenario_id,
            outcome,
            acceptance: state.acceptance,
            journal: state.journal,
            artifacts: state.artifacts,
            projection: state.projection,
            corruption: state.corruption,
            ownership: accounting.ownership,
            retries: accounting.retries,
            resources: accounting.resources,
            temporary_objects: state.temporary_objects,
            evidence,
            milestones,
        })
    }

    /// Returns the scenario identity.
    #[must_use]
    pub const fn scenario_id(&self) -> &ScenarioId {
        &self.scenario_id
    }
    /// Returns the recovered authoritative classification.
    #[must_use]
    pub const fn outcome(&self) -> RecoveryOutcome {
        self.outcome
    }
    /// Returns acceptance truth.
    #[must_use]
    pub const fn acceptance(&self) -> AcceptanceObservation {
        self.acceptance
    }
    /// Returns journal integrity.
    #[must_use]
    pub const fn journal(&self) -> JournalHealth {
        self.journal
    }
    /// Returns referenced-object integrity.
    #[must_use]
    pub const fn artifacts(&self) -> ArtifactHealth {
        self.artifacts
    }
    /// Returns projection integrity.
    #[must_use]
    pub const fn projection(&self) -> ProjectionHealth {
        self.projection
    }
    /// Returns corruption/admission facts.
    #[must_use]
    pub const fn corruption(&self) -> CorruptionObservation {
        self.corruption
    }
    /// Returns ownership reconciliation facts.
    #[must_use]
    pub const fn ownership(&self) -> OwnershipObservation {
        self.ownership
    }
    /// Returns retry counters.
    #[must_use]
    pub const fn retries(&self) -> RetryUsage {
        self.retries
    }
    /// Returns resource counters.
    #[must_use]
    pub const fn resources(&self) -> ResourceUsage {
        self.resources
    }
    /// Returns temporary objects remaining after recovery.
    #[must_use]
    pub const fn temporary_objects(&self) -> u16 {
        self.temporary_objects
    }
    /// Returns anchors in canonical kind/identifier order.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceAnchor] {
        &self.evidence
    }
    /// Returns explicitly ordered lifecycle milestones.
    #[must_use]
    pub fn milestones(&self) -> &[Milestone] {
        &self.milestones
    }
}
