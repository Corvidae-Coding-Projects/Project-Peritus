//! Restart-consumable evaluation metrics and canonical snapshot identity.

use peritus_eval::{
    EvaluationAnalysis, MetricAvailability, MetricUnavailableReason, ResultDigest, TaskId,
};
use peritus_types::Sha256Digest;

use crate::identity::digest_parts;

/// One available value or the exact E3 reason it was unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationMetric<T> {
    /// E3 produced a checked value.
    Available(T),
    /// E3 could not produce the value for this exact reason.
    Unavailable(MetricUnavailableReason),
}

impl<T: Copy> EvaluationMetric<T> {
    /// Returns the available value.
    #[must_use]
    pub const fn value(self) -> Option<T> {
        match self {
            Self::Available(value) => Some(value),
            Self::Unavailable(_) => None,
        }
    }

    /// Returns the exact unavailable reason.
    #[must_use]
    pub const fn unavailable_reason(self) -> Option<MetricUnavailableReason> {
        match self {
            Self::Available(_) => None,
            Self::Unavailable(reason) => Some(reason),
        }
    }
}

/// One exact task-level pass-at-k value consumed by F0 attribution.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaskPassAtKSnapshot {
    task_id: TaskId,
    k: u16,
    estimate_millionths: u32,
}

impl TaskPassAtKSnapshot {
    pub(crate) const fn new(task_id: TaskId, k: u16, estimate_millionths: u32) -> Self {
        Self { task_id, k, estimate_millionths }
    }
    /// Exact E3 task identity.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
    /// Requested pass-at-k value.
    #[must_use]
    pub const fn k(self) -> u16 {
        self.k
    }
    /// Deterministic probability estimate in integer millionths.
    #[must_use]
    pub const fn estimate_millionths(self) -> u32 {
        self.estimate_millionths
    }
}

/// Restart-consumable projection of every E3 analysis value used by F0 decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationAnalysisSnapshot {
    source_digest: ResultDigest,
    candidate_correctness_lower: EvaluationMetric<u32>,
    candidate_pass_at_k: EvaluationMetric<Vec<TaskPassAtKSnapshot>>,
    paired_effect_lower: EvaluationMetric<i32>,
    candidate_safety_failures: u32,
    reliability_lower: EvaluationMetric<u32>,
    latency_p95_micros: EvaluationMetric<u64>,
    cost_mean_microunits: EvaluationMetric<u64>,
    input_tokens_mean: EvaluationMetric<u64>,
    output_tokens_mean: EvaluationMetric<u64>,
    expected_rollouts: u32,
    complete_trace_rollouts: u32,
    complete_teardown_rollouts: u32,
    digest: Sha256Digest,
}

impl EvaluationAnalysisSnapshot {
    pub(super) fn capture(analysis: &EvaluationAnalysis) -> Self {
        let pass_at_k = match analysis.candidate().pass_at_k() {
            MetricAvailability::Available(tasks) => EvaluationMetric::Available(
                tasks
                    .iter()
                    .flat_map(|task| {
                        task.values().iter().map(move |metric| {
                            TaskPassAtKSnapshot::new(
                                task.task_id(),
                                metric.k(),
                                metric.estimate().get(),
                            )
                        })
                    })
                    .collect(),
            ),
            MetricAvailability::Unavailable(reason) => EvaluationMetric::Unavailable(*reason),
        };
        let reliability = analysis.reliability();
        Self::from_exact_parts(
            analysis.digest(),
            probability(analysis.candidate().raw_success_interval(), |value| value.lower().get()),
            pass_at_k,
            signed(analysis.paired(), |value| value.primary_interval().lower_millionths()),
            analysis.candidate().safety_failures(),
            probability(&reliability.evaluated_interval(), |value| value.lower().get()),
            quantity(
                analysis.candidate_resources().elapsed_micros(),
                peritus_eval::DistributionSummary::p95,
            ),
            quantity(
                analysis.candidate_resources().cost_microunits(),
                peritus_eval::DistributionSummary::mean,
            ),
            quantity(
                analysis.candidate_resources().input_tokens(),
                peritus_eval::DistributionSummary::mean,
            ),
            quantity(
                analysis.candidate_resources().output_tokens(),
                peritus_eval::DistributionSummary::mean,
            ),
            reliability.counts().expected,
            reliability.complete_trace_rollouts(),
            reliability.complete_teardown_rollouts(),
        )
    }

    #[allow(clippy::too_many_arguments, reason = "every replayed E3 decision fact stays explicit")]
    pub(crate) fn from_exact_parts(
        source_digest: ResultDigest,
        candidate_correctness_lower: EvaluationMetric<u32>,
        candidate_pass_at_k: EvaluationMetric<Vec<TaskPassAtKSnapshot>>,
        paired_effect_lower: EvaluationMetric<i32>,
        candidate_safety_failures: u32,
        reliability_lower: EvaluationMetric<u32>,
        latency_p95_micros: EvaluationMetric<u64>,
        cost_mean_microunits: EvaluationMetric<u64>,
        input_tokens_mean: EvaluationMetric<u64>,
        output_tokens_mean: EvaluationMetric<u64>,
        expected_rollouts: u32,
        complete_trace_rollouts: u32,
        complete_teardown_rollouts: u32,
    ) -> Self {
        let mut value = Self {
            source_digest,
            candidate_correctness_lower,
            candidate_pass_at_k,
            paired_effect_lower,
            candidate_safety_failures,
            reliability_lower,
            latency_p95_micros,
            cost_mean_microunits,
            input_tokens_mean,
            output_tokens_mean,
            expected_rollouts,
            complete_trace_rollouts,
            complete_teardown_rollouts,
            digest: Sha256Digest::new([0; 32]),
        };
        value.digest = analysis_snapshot_digest(&value);
        value
    }

    /// Original complete E3 analysis digest.
    #[must_use]
    pub const fn source_digest(&self) -> ResultDigest {
        self.source_digest
    }
    /// Candidate raw correctness lower bound.
    #[must_use]
    pub const fn candidate_correctness_lower(&self) -> EvaluationMetric<u32> {
        self.candidate_correctness_lower
    }
    /// Task-level candidate pass-at-k observations.
    #[must_use]
    pub const fn candidate_pass_at_k(&self) -> &EvaluationMetric<Vec<TaskPassAtKSnapshot>> {
        &self.candidate_pass_at_k
    }
    /// Candidate-minus-baseline paired lower bound.
    #[must_use]
    pub const fn paired_effect_lower(&self) -> EvaluationMetric<i32> {
        self.paired_effect_lower
    }
    /// Candidate evaluator safety failures.
    #[must_use]
    pub const fn candidate_safety_failures(&self) -> u32 {
        self.candidate_safety_failures
    }
    /// Valid-evaluator reliability lower bound.
    #[must_use]
    pub const fn reliability_lower(&self) -> EvaluationMetric<u32> {
        self.reliability_lower
    }
    /// Candidate end-to-end p95 latency.
    #[must_use]
    pub const fn latency_p95_micros(&self) -> EvaluationMetric<u64> {
        self.latency_p95_micros
    }
    /// Candidate mean cost.
    #[must_use]
    pub const fn cost_mean_microunits(&self) -> EvaluationMetric<u64> {
        self.cost_mean_microunits
    }
    /// Candidate mean input tokens.
    #[must_use]
    pub const fn input_tokens_mean(&self) -> EvaluationMetric<u64> {
        self.input_tokens_mean
    }
    /// Candidate mean output tokens.
    #[must_use]
    pub const fn output_tokens_mean(&self) -> EvaluationMetric<u64> {
        self.output_tokens_mean
    }
    /// Planned rollout population used for completeness.
    #[must_use]
    pub const fn expected_rollouts(&self) -> u32 {
        self.expected_rollouts
    }
    /// Rollouts with complete traces.
    #[must_use]
    pub const fn complete_trace_rollouts(&self) -> u32 {
        self.complete_trace_rollouts
    }
    /// Rollouts with complete teardown.
    #[must_use]
    pub const fn complete_teardown_rollouts(&self) -> u32 {
        self.complete_teardown_rollouts
    }
    /// Digest of every retained F0 analysis decision fact.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn probability<T: Copy>(
    value: &MetricAvailability<T>,
    project: impl FnOnce(T) -> u32,
) -> EvaluationMetric<u32> {
    match value {
        MetricAvailability::Available(value) => EvaluationMetric::Available(project(*value)),
        MetricAvailability::Unavailable(reason) => EvaluationMetric::Unavailable(*reason),
    }
}

fn signed<T: Copy>(
    value: &MetricAvailability<T>,
    project: impl FnOnce(T) -> i32,
) -> EvaluationMetric<i32> {
    match value {
        MetricAvailability::Available(value) => EvaluationMetric::Available(project(*value)),
        MetricAvailability::Unavailable(reason) => EvaluationMetric::Unavailable(*reason),
    }
}

fn quantity<T: Copy>(
    value: &MetricAvailability<T>,
    project: impl FnOnce(T) -> u64,
) -> EvaluationMetric<u64> {
    match value {
        MetricAvailability::Available(value) => EvaluationMetric::Available(project(*value)),
        MetricAvailability::Unavailable(reason) => EvaluationMetric::Unavailable(*reason),
    }
}

fn analysis_snapshot_digest(value: &EvaluationAnalysisSnapshot) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(value.source_digest.as_bytes());
    append_metric_u32(&mut bytes, value.candidate_correctness_lower);
    match &value.candidate_pass_at_k {
        EvaluationMetric::Available(values) => {
            bytes.push(1);
            for item in values {
                bytes.extend_from_slice(item.task_id().as_bytes());
                bytes.extend_from_slice(&item.k().to_be_bytes());
                bytes.extend_from_slice(&item.estimate_millionths().to_be_bytes());
            }
        }
        EvaluationMetric::Unavailable(reason) => bytes.extend_from_slice(&[2, reason_tag(*reason)]),
    }
    match value.paired_effect_lower {
        EvaluationMetric::Available(item) => {
            bytes.push(1);
            bytes.extend_from_slice(&item.to_be_bytes());
        }
        EvaluationMetric::Unavailable(reason) => bytes.extend_from_slice(&[2, reason_tag(reason)]),
    }
    bytes.extend_from_slice(&value.candidate_safety_failures.to_be_bytes());
    append_metric_u32(&mut bytes, value.reliability_lower);
    for metric in [
        value.latency_p95_micros,
        value.cost_mean_microunits,
        value.input_tokens_mean,
        value.output_tokens_mean,
    ] {
        match metric {
            EvaluationMetric::Available(item) => {
                bytes.push(1);
                bytes.extend_from_slice(&item.to_be_bytes());
            }
            EvaluationMetric::Unavailable(reason) => {
                bytes.extend_from_slice(&[2, reason_tag(reason)]);
            }
        }
    }
    bytes.extend_from_slice(&value.expected_rollouts.to_be_bytes());
    bytes.extend_from_slice(&value.complete_trace_rollouts.to_be_bytes());
    bytes.extend_from_slice(&value.complete_teardown_rollouts.to_be_bytes());
    digest_parts(b"peritus.f0.evaluation-analysis-snapshot.v1\0", &[&bytes])
}

fn append_metric_u32(bytes: &mut Vec<u8>, value: EvaluationMetric<u32>) {
    match value {
        EvaluationMetric::Available(item) => {
            bytes.push(1);
            bytes.extend_from_slice(&item.to_be_bytes());
        }
        EvaluationMetric::Unavailable(reason) => bytes.extend_from_slice(&[2, reason_tag(reason)]),
    }
}

pub(crate) const fn reason_tag(reason: MetricUnavailableReason) -> u8 {
    match reason {
        MetricUnavailableReason::IncompleteLedger => 1,
        MetricUnavailableReason::CancelledRollout => 2,
        MetricUnavailableReason::AmbiguousRollout => 3,
        MetricUnavailableReason::InfrastructureInvalidated => 4,
        MetricUnavailableReason::EmptyDenominator => 5,
        MetricUnavailableReason::RequiredObservationMissing => 6,
    }
}
