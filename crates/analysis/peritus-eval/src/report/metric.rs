//! Report metric values and explicit unavailability reasons.

use crate::{
    BootstrapInterval, DistributionSummary, LedgerCounts, PairedComparison, PassAtK,
    StabilitySummary, TaskId, WilsonInterval,
};

/// Closed reason why a configured metric has no value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricUnavailableReason {
    /// The expected rollout ledger is not complete.
    IncompleteLedger,
    /// Durable cancellation affects the metric population.
    CancelledRollout,
    /// An external result remained ambiguous.
    AmbiguousRollout,
    /// Frozen infrastructure policy invalidated the metric.
    InfrastructureInvalidated,
    /// No valid included observations remain.
    EmptyDenominator,
    /// A required resource/usage observation was absent.
    RequiredObservationMissing,
}

impl MetricUnavailableReason {
    pub(super) const fn tag(self) -> u8 {
        match self {
            Self::IncompleteLedger => 1,
            Self::CancelledRollout => 2,
            Self::AmbiguousRollout => 3,
            Self::InfrastructureInvalidated => 4,
            Self::EmptyDenominator => 5,
            Self::RequiredObservationMissing => 6,
        }
    }
}

/// A metric is either present or absent for one exact visible reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricAvailability<T> {
    /// Complete computed value.
    Available(T),
    /// Explicit unavailable result.
    Unavailable(MetricUnavailableReason),
}

impl<T> MetricAvailability<T> {
    /// Returns the computed value when available.
    #[must_use]
    pub const fn value(&self) -> Option<&T> {
        match self {
            Self::Available(value) => Some(value),
            Self::Unavailable(_) => None,
        }
    }
    /// Returns the visible reason when unavailable.
    #[must_use]
    pub const fn unavailable_reason(&self) -> Option<MetricUnavailableReason> {
        match self {
            Self::Available(_) => None,
            Self::Unavailable(reason) => Some(*reason),
        }
    }
}

/// Per-task pass@k vectors for one arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskPassAtK {
    pub(super) task_id: TaskId,
    pub(super) values: Vec<PassAtK>,
}

impl TaskPassAtK {
    /// Task identity.
    #[must_use]
    pub const fn task_id(&self) -> TaskId {
        self.task_id
    }
    /// Ascending configured k values.
    #[must_use]
    pub fn values(&self) -> &[PassAtK] {
        &self.values
    }
}

/// Per-task ordered-rollout stability for one arm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskStability {
    pub(super) task_id: TaskId,
    pub(super) summary: StabilitySummary,
}

impl TaskStability {
    /// Task identity.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
    /// Complete stability summary.
    #[must_use]
    pub const fn summary(self) -> StabilitySummary {
        self.summary
    }
}

/// Correctness and safety accounting for one evaluation arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArmCorrectness {
    pub(super) raw: LedgerCounts,
    pub(super) safety_failures: u32,
    pub(super) excluded_infrastructure: u32,
    pub(super) raw_success_interval: MetricAvailability<WilsonInterval>,
    pub(super) pass_at_k: MetricAvailability<Vec<TaskPassAtK>>,
    pub(super) stability: MetricAvailability<Vec<TaskStability>>,
}

impl ArmCorrectness {
    /// Raw conserved arm counts.
    #[must_use]
    pub const fn raw(&self) -> LedgerCounts {
        self.raw
    }
    /// Valid evaluator safety failures.
    #[must_use]
    pub const fn safety_failures(&self) -> u32 {
        self.safety_failures
    }
    /// Infrastructure terminals excluded by visible policy.
    #[must_use]
    pub const fn excluded_infrastructure(&self) -> u32 {
        self.excluded_infrastructure
    }
    /// Wilson interval for the raw included success proportion.
    #[must_use]
    pub const fn raw_success_interval(&self) -> &MetricAvailability<WilsonInterval> {
        &self.raw_success_interval
    }
    /// Per-task pass@k metrics.
    #[must_use]
    pub const fn pass_at_k(&self) -> &MetricAvailability<Vec<TaskPassAtK>> {
        &self.pass_at_k
    }
    /// Per-task ordered stability metrics.
    #[must_use]
    pub const fn stability(&self) -> &MetricAvailability<Vec<TaskStability>> {
        &self.stability
    }
}

/// Paired candidate/baseline result with invalid-pair accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PairedEvidence {
    pub(super) comparison: PairedComparison,
    pub(super) invalid_pairs: u32,
}

impl PairedEvidence {
    /// Complete valid-pair statistical comparison.
    #[must_use]
    pub const fn comparison(self) -> PairedComparison {
        self.comparison
    }
    /// Pairs excluded due to visible non-task outcomes.
    #[must_use]
    pub const fn invalid_pairs(self) -> u32 {
        self.invalid_pairs
    }
    /// Primary task-cluster bootstrap interval.
    #[must_use]
    pub const fn primary_interval(self) -> BootstrapInterval {
        self.comparison.interval()
    }
}

/// Latency, cost, usage, CPU, and memory summaries for one arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArmResourceSummary {
    pub(super) elapsed_micros: MetricAvailability<DistributionSummary>,
    pub(super) cost_microunits: MetricAvailability<DistributionSummary>,
    pub(super) input_tokens: MetricAvailability<DistributionSummary>,
    pub(super) output_tokens: MetricAvailability<DistributionSummary>,
    pub(super) cpu_micros: MetricAvailability<DistributionSummary>,
    pub(super) memory_high_water_bytes: MetricAvailability<DistributionSummary>,
}

impl ArmResourceSummary {
    /// End-to-end observed stage latency.
    #[must_use]
    pub const fn elapsed_micros(&self) -> &MetricAvailability<DistributionSummary> {
        &self.elapsed_micros
    }
    /// Provider cost in microunits.
    #[must_use]
    pub const fn cost_microunits(&self) -> &MetricAvailability<DistributionSummary> {
        &self.cost_microunits
    }
    /// Provider input tokens.
    #[must_use]
    pub const fn input_tokens(&self) -> &MetricAvailability<DistributionSummary> {
        &self.input_tokens
    }
    /// Provider output tokens.
    #[must_use]
    pub const fn output_tokens(&self) -> &MetricAvailability<DistributionSummary> {
        &self.output_tokens
    }
    /// Observed CPU microseconds.
    #[must_use]
    pub const fn cpu_micros(&self) -> &MetricAvailability<DistributionSummary> {
        &self.cpu_micros
    }
    /// Observed memory high-water bytes.
    #[must_use]
    pub const fn memory_high_water_bytes(&self) -> &MetricAvailability<DistributionSummary> {
        &self.memory_high_water_bytes
    }
}

/// Raw reliability and observation-completeness evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationReliability {
    pub(super) counts: LedgerCounts,
    pub(super) attempts: u32,
    pub(super) retried_rollouts: u32,
    pub(super) complete_trace_rollouts: u32,
    pub(super) complete_teardown_rollouts: u32,
    pub(super) evaluated_interval: MetricAvailability<WilsonInterval>,
    pub(super) infrastructure_interval: MetricAvailability<WilsonInterval>,
}

impl EvaluationReliability {
    /// Complete campaign terminal counts.
    #[must_use]
    pub const fn counts(self) -> LedgerCounts {
        self.counts
    }
    /// All retained attempts.
    #[must_use]
    pub const fn attempts(self) -> u32 {
        self.attempts
    }
    /// Logical rollouts with more than one retained attempt.
    #[must_use]
    pub const fn retried_rollouts(self) -> u32 {
        self.retried_rollouts
    }
    /// Rollouts whose executed stages reported complete traces.
    #[must_use]
    pub const fn complete_trace_rollouts(self) -> u32 {
        self.complete_trace_rollouts
    }
    /// Rollouts whose executed stages reported complete teardown.
    #[must_use]
    pub const fn complete_teardown_rollouts(self) -> u32 {
        self.complete_teardown_rollouts
    }
    /// Wilson interval for valid evaluator completion.
    #[must_use]
    pub const fn evaluated_interval(self) -> MetricAvailability<WilsonInterval> {
        self.evaluated_interval
    }
    /// Wilson interval for infrastructure failures.
    #[must_use]
    pub const fn infrastructure_interval(self) -> MetricAvailability<WilsonInterval> {
        self.infrastructure_interval
    }
}
