//! Public metric-summary and qualification-verdict report types.

use serde::Serialize;

use crate::{
    AccountingSummary, Metric, ObjectiveBound, QualificationError, RegressionResult, RunnerReceipt,
    RunnerTermination, StableId, Statistic,
};

/// Complete integer statistics for one workload and metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MetricSummary {
    workload_id: StableId,
    metric: Metric,
    sample_count: usize,
    minimum: u64,
    mean: u64,
    p50: u64,
    p95: u64,
    p99: u64,
    maximum: u64,
    total: u64,
}

impl MetricSummary {
    pub(crate) fn from_values(
        workload_id: StableId,
        metric: Metric,
        mut values: Vec<u64>,
    ) -> Result<Self, QualificationError> {
        values.sort_unstable();
        let minimum = values[0];
        let maximum = values[values.len() - 1];
        let total = values.iter().try_fold(0_u64, |sum, value| {
            sum.checked_add(*value)
                .ok_or(QualificationError::ArithmeticOverflow("measurement total"))
        })?;
        let divisor = u64::try_from(values.len())
            .map_err(|_| QualificationError::ArithmeticOverflow("measurement sample count"))?;
        let mean = total / divisor;
        Ok(Self {
            workload_id,
            metric,
            sample_count: values.len(),
            minimum,
            mean,
            p50: percentile(&values, 50),
            p95: percentile(&values, 95),
            p99: percentile(&values, 99),
            maximum,
            total,
        })
    }

    /// Returns the workload binding.
    #[must_use]
    pub const fn workload_id(&self) -> &StableId {
        &self.workload_id
    }

    /// Returns the summarized metric.
    #[must_use]
    pub const fn metric(&self) -> Metric {
        self.metric
    }

    /// Returns the number of observations.
    #[must_use]
    pub const fn sample_count(&self) -> usize {
        self.sample_count
    }

    /// Selects one stored statistic.
    #[must_use]
    pub const fn value(&self, statistic: Statistic) -> u64 {
        match statistic {
            Statistic::Minimum => self.minimum,
            Statistic::Mean => self.mean,
            Statistic::P50 => self.p50,
            Statistic::P95 => self.p95,
            Statistic::P99 => self.p99,
            Statistic::Maximum => self.maximum,
            Statistic::Total => self.total,
        }
    }
}

/// Result of evaluating one SLO objective.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveStatus {
    /// Sufficient evidence met the inclusive threshold.
    Met,
    /// Sufficient evidence missed the inclusive threshold.
    Missed,
    /// No matching summary or too few observations existed.
    InsufficientEvidence,
}

/// Report entry for one service-level objective.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ObjectiveEvaluation {
    pub(crate) objective_id: StableId,
    pub(crate) workload_id: StableId,
    pub(crate) metric: Metric,
    pub(crate) statistic: Statistic,
    pub(crate) bound: ObjectiveBound,
    pub(crate) threshold: u64,
    pub(crate) observed: Option<u64>,
    pub(crate) sample_count: usize,
    pub(crate) status: ObjectiveStatus,
}

impl ObjectiveEvaluation {
    /// Returns the stable objective identifier.
    #[must_use]
    pub const fn objective_id(&self) -> &StableId {
        &self.objective_id
    }

    /// Returns the workload whose statistic was evaluated.
    #[must_use]
    pub const fn workload_id(&self) -> &StableId {
        &self.workload_id
    }

    /// Returns the metric selected by this objective.
    #[must_use]
    pub const fn metric(&self) -> Metric {
        self.metric
    }

    /// Returns the summary statistic selected by this objective.
    #[must_use]
    pub const fn statistic(&self) -> Statistic {
        self.statistic
    }

    /// Returns the objective status.
    #[must_use]
    pub const fn status(&self) -> ObjectiveStatus {
        self.status
    }

    /// Returns the selected statistic when sufficient samples existed.
    #[must_use]
    pub const fn observed(&self) -> Option<u64> {
        self.observed
    }

    /// Returns matching sample count.
    #[must_use]
    pub const fn sample_count(&self) -> usize {
        self.sample_count
    }
}

/// Structured reason preventing a production-ready H3 verdict.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum NotReadyReason {
    /// A profile-required workload definition was absent.
    MissingWorkloadDefinition {
        /// Missing workload identifier.
        workload_id: StableId,
    },
    /// No terminal runner receipt existed for a required workload.
    MissingRunnerReceipt {
        /// Missing workload identifier.
        workload_id: StableId,
    },
    /// A required workload runner stopped without executing every step.
    RunnerIncomplete {
        /// Workload identifier.
        workload_id: StableId,
        /// Reported terminal state.
        termination: RunnerTermination,
    },
    /// A runner receipt understated or overstated the stable workload schedule.
    RunnerPlanMismatch {
        /// Workload identifier.
        workload_id: StableId,
        /// Stable workload operation count.
        expected: u64,
        /// Operation count claimed by the receipt.
        observed: u64,
    },
    /// No measurements existed for a required workload.
    MissingMeasurements {
        /// Missing workload identifier.
        workload_id: StableId,
    },
    /// An objective did not have enough samples.
    InsufficientObjectiveEvidence {
        /// Objective identifier.
        objective_id: StableId,
    },
    /// An objective missed its threshold.
    ObjectiveMissed {
        /// Objective identifier.
        objective_id: StableId,
    },
    /// The accounting ledger retained active lifecycle resources or queued items.
    UnbalancedResources,
    /// A workload did not exercise the concurrency or queue level it declared.
    ResourceExerciseMissing {
        /// Workload identifier.
        workload_id: StableId,
        /// Resource category.
        resource: &'static str,
        /// Required observed high-water value.
        expected: u64,
        /// Actual high-water value.
        observed: u64,
    },
    /// Regression policy required an exact baseline entry that was absent.
    RequiredBaselineMissing {
        /// Objective identifier.
        objective_id: StableId,
    },
    /// Candidate performance crossed the blocking regression threshold.
    BlockingRegression {
        /// Objective identifier.
        objective_id: StableId,
    },
}

/// Derived H3 gate verdict. This does not perform a release transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationVerdict {
    /// Every required workload, objective, runner, resource check, and baseline policy passed.
    Ready,
    /// At least one structured blocker remains.
    NotReady,
}

/// Complete deterministic result of a qualification evaluation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QualificationEvaluation {
    pub(crate) profile_id: StableId,
    pub(crate) run_id: StableId,
    pub(crate) summaries: Vec<MetricSummary>,
    pub(crate) objectives: Vec<ObjectiveEvaluation>,
    pub(crate) regressions: Vec<RegressionResult>,
    pub(crate) accounting: AccountingSummary,
    pub(crate) runner_receipts: Vec<RunnerReceipt>,
    pub(crate) verdict: QualificationVerdict,
    pub(crate) not_ready_reasons: Vec<NotReadyReason>,
}

impl QualificationEvaluation {
    /// Returns the profile binding.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile_id
    }

    /// Returns the qualification run binding.
    #[must_use]
    pub const fn run_id(&self) -> &StableId {
        &self.run_id
    }

    /// Returns stable workload/metric summaries.
    #[must_use]
    pub fn summaries(&self) -> &[MetricSummary] {
        &self.summaries
    }

    /// Returns objective results in profile order.
    #[must_use]
    pub fn objectives(&self) -> &[ObjectiveEvaluation] {
        &self.objectives
    }

    /// Returns baseline comparisons in objective order.
    #[must_use]
    pub fn regressions(&self) -> &[RegressionResult] {
        &self.regressions
    }

    /// Returns bounded resource accounting evidence.
    #[must_use]
    pub const fn accounting(&self) -> &AccountingSummary {
        &self.accounting
    }

    /// Returns the derived H3 readiness verdict.
    #[must_use]
    pub const fn verdict(&self) -> QualificationVerdict {
        self.verdict
    }

    /// Returns every blocker in deterministic discovery order.
    #[must_use]
    pub fn not_ready_reasons(&self) -> &[NotReadyReason] {
        &self.not_ready_reasons
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let numerator = sorted.len().saturating_mul(percentile);
    let rank = numerator.saturating_add(99) / 100;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}
