//! Performance metrics and statistically scoped service-level objectives.

use serde::{Deserialize, Serialize};

use crate::{QualificationError, StableId};

/// Unit carried by every observation of a metric.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    /// Microseconds.
    Microseconds,
    /// Bytes.
    Bytes,
    /// Bytes completed per second.
    BytesPerSecond,
    /// Discrete operations completed per second.
    OperationsPerSecond,
    /// Tokens consumed or emitted per second.
    TokensPerSecond,
    /// A discrete count.
    Count,
    /// Hundredths of one percent, in the inclusive range 0 through 10,000.
    BasisPoints,
}

/// Whether a smaller or larger observation is preferable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricDirection {
    /// Smaller values are preferable.
    LowerIsBetter,
    /// Larger values are preferable.
    HigherIsBetter,
}

/// Metrics admitted by H3 qualification datasets.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Time from daemon invocation to readiness.
    DaemonStartupLatency,
    /// Time from restart to reconciled service readiness.
    RecoveryLatency,
    /// Time from command receipt to its first durable event.
    CommandToFirstEventLatency,
    /// Duration of one authoritative event append.
    EventAppendLatency,
    /// Time from cancellation request to owned process-tree termination.
    CancellationLatency,
    /// Maximum observed artifact garbage-collection pause.
    ArtifactGcPause,
    /// Time an operation waits because a bounded queue is saturated.
    QueueSaturationWait,
    /// Time an operation remains backpressured by a provider.
    ProviderBackpressureLatency,
    /// Time an operation remains backpressured by a telemetry exporter.
    ExporterBackpressureLatency,
    /// Sustained terminal output bytes delivered per second.
    TerminalThroughput,
    /// Projection events rebuilt per second.
    ProjectionRebuildThroughput,
    /// Owned processes completed per second.
    ProcessThroughput,
    /// Model tokens accounted per second.
    TokenThroughput,
    /// Artifact bytes durably streamed per second.
    DiskThroughput,
    /// Steady-state resident bytes attributed to one active run.
    SteadyMemoryPerRun,
    /// Steady-state resident bytes attributed to one streamed process.
    SteadyMemoryPerProcess,
    /// Peak resident bytes for the subject process tree.
    PeakResidentMemory,
    /// Simultaneously active runs.
    ConcurrentRuns,
    /// Simultaneously active owned processes.
    ConcurrentProcesses,
    /// Simultaneously active provider requests.
    ConcurrentProviderRequests,
    /// Depth of the authoritative command queue.
    CommandQueueDepth,
    /// Depth of the terminal delivery queue.
    TerminalQueueDepth,
    /// Depth of the telemetry exporter queue.
    ExporterQueueDepth,
    /// Depth of the provider request queue.
    ProviderQueueDepth,
    /// Bytes retained on disk by the active qualification run.
    DiskUsage,
    /// Tokens consumed by the active qualification run.
    TokensConsumed,
    /// Successful recoveries as basis points of recovery attempts.
    RecoverySuccessRatio,
    /// Successful cancellations as basis points of cancellation attempts.
    CancellationSuccessRatio,
}

impl Metric {
    /// Returns the unit required for observations of this metric.
    #[must_use]
    pub const fn unit(self) -> MetricUnit {
        match self {
            Self::DaemonStartupLatency
            | Self::RecoveryLatency
            | Self::CommandToFirstEventLatency
            | Self::EventAppendLatency
            | Self::CancellationLatency
            | Self::ArtifactGcPause
            | Self::QueueSaturationWait
            | Self::ProviderBackpressureLatency
            | Self::ExporterBackpressureLatency => MetricUnit::Microseconds,
            Self::TerminalThroughput | Self::DiskThroughput => MetricUnit::BytesPerSecond,
            Self::ProjectionRebuildThroughput | Self::ProcessThroughput => {
                MetricUnit::OperationsPerSecond
            }
            Self::TokenThroughput => MetricUnit::TokensPerSecond,
            Self::SteadyMemoryPerRun
            | Self::SteadyMemoryPerProcess
            | Self::PeakResidentMemory
            | Self::DiskUsage => MetricUnit::Bytes,
            Self::ConcurrentRuns
            | Self::ConcurrentProcesses
            | Self::ConcurrentProviderRequests
            | Self::CommandQueueDepth
            | Self::TerminalQueueDepth
            | Self::ExporterQueueDepth
            | Self::ProviderQueueDepth
            | Self::TokensConsumed => MetricUnit::Count,
            Self::RecoverySuccessRatio | Self::CancellationSuccessRatio => MetricUnit::BasisPoints,
        }
    }

    /// Returns the direction used for SLO and regression comparisons.
    #[must_use]
    pub const fn direction(self) -> MetricDirection {
        match self {
            Self::TerminalThroughput
            | Self::ProjectionRebuildThroughput
            | Self::ProcessThroughput
            | Self::TokenThroughput
            | Self::DiskThroughput
            | Self::ConcurrentRuns
            | Self::ConcurrentProcesses
            | Self::ConcurrentProviderRequests
            | Self::RecoverySuccessRatio
            | Self::CancellationSuccessRatio => MetricDirection::HigherIsBetter,
            Self::DaemonStartupLatency
            | Self::RecoveryLatency
            | Self::CommandToFirstEventLatency
            | Self::EventAppendLatency
            | Self::CancellationLatency
            | Self::ArtifactGcPause
            | Self::QueueSaturationWait
            | Self::ProviderBackpressureLatency
            | Self::ExporterBackpressureLatency
            | Self::SteadyMemoryPerRun
            | Self::SteadyMemoryPerProcess
            | Self::PeakResidentMemory
            | Self::CommandQueueDepth
            | Self::TerminalQueueDepth
            | Self::ExporterQueueDepth
            | Self::ProviderQueueDepth
            | Self::DiskUsage
            | Self::TokensConsumed => MetricDirection::LowerIsBetter,
        }
    }

    pub(crate) fn validate_value(self, value: u64) -> Result<(), QualificationError> {
        if self.unit() == MetricUnit::BasisPoints && value > 10_000 {
            return Err(QualificationError::invalid_value(
                "measurement.value",
                "basis-point observations must not exceed 10,000",
            ));
        }
        Ok(())
    }
}

/// Statistic selected from all observations matching an objective.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Statistic {
    /// Minimum observation.
    Minimum,
    /// Arithmetic mean rounded down to the metric's integer unit.
    Mean,
    /// Nearest-rank 50th percentile.
    P50,
    /// Nearest-rank 95th percentile.
    P95,
    /// Nearest-rank 99th percentile.
    P99,
    /// Maximum observation.
    Maximum,
    /// Sum of all observations.
    Total,
}

/// Inclusive comparison used by an SLO objective.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveBound {
    /// The selected statistic must not exceed the threshold.
    AtMost,
    /// The selected statistic must be at least the threshold.
    AtLeast,
}

/// One workload-scoped service-level objective.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SloObjective {
    id: StableId,
    workload_id: StableId,
    metric: Metric,
    statistic: Statistic,
    bound: ObjectiveBound,
    threshold: u64,
    minimum_samples: usize,
}

impl SloObjective {
    /// Constructs an objective and rejects a bound contrary to the metric's improvement direction.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the sample minimum is zero, the threshold is invalid
    /// for the metric unit, or the bound contradicts the metric's improvement direction.
    pub fn new(
        id: StableId,
        workload_id: StableId,
        metric: Metric,
        statistic: Statistic,
        bound: ObjectiveBound,
        threshold: u64,
        minimum_samples: usize,
    ) -> Result<Self, QualificationError> {
        if minimum_samples == 0 {
            return Err(QualificationError::invalid_value(
                "objective.minimum_samples",
                "must be greater than zero",
            ));
        }
        metric.validate_value(threshold)?;
        let expected = match metric.direction() {
            MetricDirection::LowerIsBetter => ObjectiveBound::AtMost,
            MetricDirection::HigherIsBetter => ObjectiveBound::AtLeast,
        };
        if bound != expected {
            return Err(QualificationError::invalid_value(
                "objective.bound",
                "must agree with the metric improvement direction",
            ));
        }
        Ok(Self { id, workload_id, metric, statistic, bound, threshold, minimum_samples })
    }

    /// Returns the objective identifier.
    #[must_use]
    pub const fn id(&self) -> &StableId {
        &self.id
    }

    /// Returns the workload binding.
    #[must_use]
    pub const fn workload_id(&self) -> &StableId {
        &self.workload_id
    }

    /// Returns the measured metric.
    #[must_use]
    pub const fn metric(&self) -> Metric {
        self.metric
    }

    /// Returns the selected statistic.
    #[must_use]
    pub const fn statistic(&self) -> Statistic {
        self.statistic
    }

    /// Returns the inclusive threshold comparison.
    #[must_use]
    pub const fn bound(&self) -> ObjectiveBound {
        self.bound
    }

    /// Returns the metric-unit threshold.
    #[must_use]
    pub const fn threshold(&self) -> u64 {
        self.threshold
    }

    /// Returns the minimum number of required observations.
    #[must_use]
    pub const fn minimum_samples(&self) -> usize {
        self.minimum_samples
    }

    /// Returns whether a selected statistic satisfies the inclusive objective.
    #[must_use]
    pub const fn accepts(&self, value: u64) -> bool {
        match self.bound {
            ObjectiveBound::AtMost => value <= self.threshold,
            ObjectiveBound::AtLeast => value >= self.threshold,
        }
    }
}
