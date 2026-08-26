//! Runtime-neutral F0 production-harness evolution conformance contract.

mod cases;

pub use cases::evolution_suite;

/// One independently exercised F0 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionScenario {
    /// Campaign and pointer inputs retain exact immutable E1/E2/E3 bindings.
    FrozenEvidence,
    /// Every declared component delta is complete and protected assets remain unchanged.
    ChangeIsolation,
    /// Interacting changes are attributed only to their declared group.
    InteractionAttribution,
    /// Contaminated, evaluator-drifted, or incomplete evidence fails closed.
    Contamination,
    /// Aggregate improvements cannot offset a mandatory regression or unavailable criterion.
    MetricGaming,
    /// Selection is stable across insertion order and supported hosts.
    DeterministicSelection,
    /// Stale baseline, evidence, policy, or schema bindings reject promotion.
    StaleEvidence,
    /// Executable changes require complete independent D2 review.
    IndependentReview,
    /// B0/B1 action, capability, approval, and currentness bindings are exact and single-use.
    HumanAuthority,
    /// Campaign terminalization, pointer activation, and approval consumption are atomic.
    AtomicActivation,
    /// Rollback appends a newly approved activation to a retained compatible revision.
    RollbackHistory,
    /// Event replay, checkpoints, artifacts, and publication recover without duplicate authority.
    DurableReplay,
    /// Unknown, malformed, noncanonical, and trailing wire input remains inert.
    MalformedInput,
    /// Independent campaign, collection, frame, and history limits fail closed.
    Bounds,
}

/// Stable terminal observed from one F0 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionTerminal {
    /// A selected variant became the production pointer.
    Promoted,
    /// The proposed operation was rejected without changing production.
    Rejected,
    /// A prior compatible revision became current through a new activation.
    RolledBack,
}

/// Fixed realistic bounds supplied to one F0 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvolutionConformanceFixture {
    scenario: EvolutionScenario,
    maximum_manifests: u16,
    maximum_variants: u16,
    maximum_criteria: u16,
    maximum_activation_history: u16,
}

impl EvolutionConformanceFixture {
    pub(crate) const fn new(scenario: EvolutionScenario) -> Self {
        Self {
            scenario,
            maximum_manifests: 64,
            maximum_variants: 32,
            maximum_criteria: 64,
            maximum_activation_history: 128,
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> EvolutionScenario {
        self.scenario
    }
    /// Returns the manifest ceiling.
    #[must_use]
    pub const fn maximum_manifests(self) -> u16 {
        self.maximum_manifests
    }
    /// Returns the variant ceiling.
    #[must_use]
    pub const fn maximum_variants(self) -> u16 {
        self.maximum_variants
    }
    /// Returns the criterion ceiling.
    #[must_use]
    pub const fn maximum_criteria(self) -> u16 {
        self.maximum_criteria
    }
    /// Returns the retained activation ceiling.
    #[must_use]
    pub const fn maximum_activation_history(self) -> u16 {
        self.maximum_activation_history
    }
}

/// Direct observations from one complete F0 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent F0 contract facts remain visible to third-party implementations"
)]
pub struct EvolutionConformanceObservation {
    /// Terminal outcome.
    pub terminal: EvolutionTerminal,
    /// Change manifests retained by the campaign.
    pub manifests: u16,
    /// Candidate variants retained by the campaign.
    pub variants: u16,
    /// Independently evaluated policy criteria.
    pub criteria: u16,
    /// Append-only production activation records retained.
    pub activation_history: u16,
    /// Exact E1/E2/E3/policy bindings remained immutable.
    pub frozen_evidence_exact: bool,
    /// Component changes were complete and protected assets isolated.
    pub change_isolation_exact: bool,
    /// Interaction groups prevented unsupported per-change attribution.
    pub interaction_attribution_exact: bool,
    /// Contaminated or incomplete evidence was rejected.
    pub contamination_rejected: bool,
    /// Mandatory failures could not be offset by favorable metrics.
    pub metric_gaming_rejected: bool,
    /// Stable selection was insertion- and host-independent.
    pub selection_deterministic: bool,
    /// Stale baselines, evidence, policies, and schemas were rejected.
    pub stale_evidence_rejected: bool,
    /// Required review was completed by an independent quorum.
    pub review_exact: bool,
    /// B0/B1 authority was exact, current, durable, and single-use.
    pub authority_exact: bool,
    /// Campaign, pointer, and approval state changed atomically.
    pub activation_atomic: bool,
    /// Rollback appended history and targeted a retained compatible revision.
    pub rollback_auditable: bool,
    /// Replay and recovery reproduced state without duplicate authority.
    pub replay_equivalent: bool,
    /// Malformed protocol input remained inert and rejected.
    pub malformed_rejected: bool,
    /// Every independent configured limit was enforced without truncation.
    pub bounds_enforced: bool,
    /// Evidence and artifacts remained exact and provenance checked.
    pub evidence_exact: bool,
    /// No report, score, or proposal could promote itself.
    pub non_self_promoting: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by an F0 production subject or development bridge.
pub trait EvolutionConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    /// Returns [`EvolutionConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &EvolutionConformanceFixture,
    ) -> Result<EvolutionConformanceObservation, EvolutionConformanceError>;
}
