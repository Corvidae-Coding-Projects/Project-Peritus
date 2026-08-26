//! Runtime-neutral E3 evaluation conformance contract.

mod cases;

pub use cases::evaluation_suite;

/// One independently exercised E3 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationScenario {
    /// Dataset, profile, provider, model, and harness bindings remain digest-pinned.
    FrozenInputs,
    /// Candidate and evaluator workspaces and credentials remain isolated.
    RolloutIsolation,
    /// Plans, seeds, schedules, and reports are reproducible.
    DeterministicCampaign,
    /// Every planned rollout settles exactly once with complete attempt history.
    CompleteAccounting,
    /// Statistical estimators enforce their preconditions and expected bounds.
    StatisticalValidity,
    /// Infrastructure failures remain distinct from valid evaluator failures.
    InfrastructureClassification,
    /// Cancellation is durable, terminal, and idempotent.
    Cancellation,
    /// Journal replay and exact retry avoid duplicate effects.
    DurableReplay,
    /// Unknown, noncanonical, and trailing wire input remains inert.
    MalformedInput,
    /// Reports and evidence publish only after finalized artifact persistence.
    PublicationOrdering,
    /// Default reports and diagnostics exclude sensitive canaries.
    Redaction,
    /// A panicking subject remains a typed failed conformance case.
    PanicContainment,
    /// Teardown failure remains visible and cannot manufacture a passing suite.
    TeardownIsolation,
}

/// Stable terminal observed from one E3 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationTerminal {
    /// A validated campaign completed and published its report.
    Completed,
    /// Invalid input or an invalid statistical/report state was rejected.
    Rejected,
    /// The campaign settled as durably cancelled.
    Cancelled,
}

/// Fixed realistic bounds supplied to one E3 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationConformanceFixture {
    scenario: EvaluationScenario,
    maximum_rollouts: u32,
    maximum_attempts_per_rollout: u16,
    maximum_report_metrics: u16,
    canary: &'static str,
}

impl EvaluationConformanceFixture {
    pub(crate) const fn new(scenario: EvaluationScenario) -> Self {
        Self {
            scenario,
            maximum_rollouts: 64,
            maximum_attempts_per_rollout: 4,
            maximum_report_metrics: 32,
            canary: "peritus-e3-sensitive-canary",
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> EvaluationScenario {
        self.scenario
    }

    /// Returns the rollout ceiling.
    #[must_use]
    pub const fn maximum_rollouts(self) -> u32 {
        self.maximum_rollouts
    }

    /// Returns the per-rollout attempt ceiling.
    #[must_use]
    pub const fn maximum_attempts_per_rollout(self) -> u16 {
        self.maximum_attempts_per_rollout
    }

    /// Returns the report metric ceiling.
    #[must_use]
    pub const fn maximum_report_metrics(self) -> u16 {
        self.maximum_report_metrics
    }

    /// Returns the sensitive canary excluded from default surfaces.
    #[must_use]
    pub const fn canary(self) -> &'static str {
        self.canary
    }
}

/// Direct observations from one complete E3 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent E3 contract facts remain visible to third-party implementations"
)]
pub struct EvaluationConformanceObservation {
    /// Terminal outcome.
    pub terminal: EvaluationTerminal,
    /// Rollouts retained by the immutable campaign plan.
    pub planned_rollouts: u32,
    /// Greatest attempt count retained for any rollout.
    pub maximum_attempts: u16,
    /// Metrics retained by the report.
    pub report_metrics: u16,
    /// All input bindings remained immutable and digest-exact.
    pub frozen_inputs_exact: bool,
    /// Candidate and evaluator capabilities remained isolated.
    pub isolation_exact: bool,
    /// Repeated construction produced identical plan, seeds, schedule, and report bytes.
    pub deterministic: bool,
    /// Planned and terminal rollout identities formed the same duplicate-free set.
    pub accounting_complete: bool,
    /// Statistical preconditions, intervals, pairing, and bounds were enforced.
    pub statistics_valid: bool,
    /// Infrastructure outcomes never became evaluator task failures.
    pub infrastructure_distinct: bool,
    /// Cancellation was durable, terminal, and idempotent.
    pub cancellation_durable: bool,
    /// Replay and exact retry reproduced state without duplicate effects.
    pub replay_equivalent: bool,
    /// Malformed protocol input remained inert and rejected.
    pub malformed_rejected: bool,
    /// Artifact finalization preceded evidence publication and report completion.
    pub publication_ordered: bool,
    /// Sensitive canaries were absent from default reports and diagnostics.
    pub redaction_safe: bool,
    /// Every independent configured limit was enforced without silent truncation.
    pub bounds_enforced: bool,
    /// Panic was contained as a case failure.
    pub panic_contained: bool,
    /// Teardown failure remained explicit and non-passing.
    pub teardown_explicit: bool,
    /// Evaluation output carried no acceptance, promotion, or deployment authority.
    pub non_authoritative: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by an E3 production subject or development bridge.
pub trait EvaluationConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`EvaluationConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &EvaluationConformanceFixture,
    ) -> Result<EvaluationConformanceObservation, EvaluationConformanceError>;
}
