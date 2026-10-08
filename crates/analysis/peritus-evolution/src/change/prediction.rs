//! Falsifiable typed change predictions.

use crate::{
    BoundedText, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    identity::digest_parts,
};
use peritus_eval::TaskId;
use peritus_types::Sha256Digest;

/// Scope to which one prediction applies.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PredictionSubject {
    /// The complete evaluation campaign.
    Campaign,
    /// One exact E3 dataset task.
    Task(TaskId),
    /// One exact E2-derived failure-class digest.
    FailureClass(Sha256Digest),
}

/// Closed E3 observations available for falsification and selection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PredictionMetric {
    /// Candidate raw-success Wilson lower bound.
    CandidateCorrectnessLower,
    /// Candidate-minus-baseline paired bootstrap lower bound.
    PairedEffectLower,
    /// Per-task pass@k estimate.
    TaskPassAtK(u16),
    /// Valid evaluator safety-failure count.
    SafetyFailures,
    /// Valid-evaluator reliability Wilson lower bound.
    ReliabilityLower,
    /// End-to-end candidate p95 latency.
    LatencyP95Micros,
    /// Candidate mean provider cost.
    CostMeanMicrounits,
    /// Candidate mean provider input tokens.
    InputTokensMean,
    /// Candidate mean provider output tokens.
    OutputTokensMean,
    /// Fraction of planned rollouts with complete traces.
    TraceCompleteness,
    /// Fraction of planned rollouts with complete teardown.
    TeardownCompleteness,
}

impl PredictionMetric {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::CandidateCorrectnessLower => 1,
            Self::PairedEffectLower => 2,
            Self::TaskPassAtK(_) => 3,
            Self::SafetyFailures => 4,
            Self::ReliabilityLower => 5,
            Self::LatencyP95Micros => 6,
            Self::CostMeanMicrounits => 7,
            Self::InputTokensMean => 8,
            Self::OutputTokensMean => 9,
            Self::TraceCompleteness => 10,
            Self::TeardownCompleteness => 11,
        }
    }
}

/// Exact integer/fixed-point metric value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MetricValue {
    /// Signed candidate-minus-baseline millionths.
    SignedMillionths(i32),
    /// Probability in closed integer millionths.
    ProbabilityMillionths(u32),
    /// Exact event or failure count.
    Count(u32),
    /// Exact nonnegative resource quantity.
    Quantity(u64),
}

impl MetricValue {
    /// Constructs checked probability millionths.
    ///
    /// # Errors
    /// Rejects values above one million.
    pub const fn probability(value: u32) -> Result<Self, EvolutionError> {
        Self::ProbabilityMillionths(value).validate()
    }

    pub(crate) const fn validate(self) -> Result<Self, EvolutionError> {
        if self.within_numeric_domain() {
            Ok(self)
        } else {
            Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "prediction probability exceeds one million",
            ))
        }
    }

    pub(crate) const fn compatible(self, metric: PredictionMetric) -> bool {
        self.within_numeric_domain()
            && matches!(
            (metric, self),
            (PredictionMetric::PairedEffectLower, Self::SignedMillionths(_))
                | (
                    PredictionMetric::CandidateCorrectnessLower
                        | PredictionMetric::TaskPassAtK(_)
                        | PredictionMetric::ReliabilityLower
                        | PredictionMetric::TraceCompleteness
                        | PredictionMetric::TeardownCompleteness,
                    Self::ProbabilityMillionths(_)
                )
                | (PredictionMetric::SafetyFailures, Self::Count(_))
                | (
                    PredictionMetric::LatencyP95Micros
                        | PredictionMetric::CostMeanMicrounits
                        | PredictionMetric::InputTokensMean
                        | PredictionMetric::OutputTokensMean,
                    Self::Quantity(_)
                )
            )
    }

    const fn within_numeric_domain(self) -> bool {
        match self {
            Self::ProbabilityMillionths(value) => value <= 1_000_000,
            Self::SignedMillionths(_) | Self::Count(_) | Self::Quantity(_) => true,
        }
    }
}

/// Expected relation between observed and declared values.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PredictionDirection {
    /// Observed value must be at least the threshold.
    AtLeast,
    /// Observed value must be at most the threshold.
    AtMost,
    /// Observed value must equal the threshold.
    Equal,
}

/// One immutable falsifiable prediction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Prediction {
    subject: PredictionSubject,
    metric: PredictionMetric,
    direction: PredictionDirection,
    threshold: MetricValue,
    rationale: BoundedText,
    mandatory: bool,
    critical: bool,
    digest: Sha256Digest,
}

impl Prediction {
    /// Constructs one typed prediction.
    ///
    /// # Errors
    /// Rejects a threshold whose numeric representation or value is outside the metric domain, or
    /// a task-scoped metric without an exact task identity. Mandatory failure-class predictions
    /// are rejected because retained E3 evidence has no class-specific observation for them.
    pub fn new(
        subject: PredictionSubject,
        metric: PredictionMetric,
        direction: PredictionDirection,
        threshold: MetricValue,
        rationale: BoundedText,
        mandatory: bool,
        critical: bool,
    ) -> Result<Self, EvolutionError> {
        let prediction = Self::from_exact_parts(
            subject, metric, direction, threshold, rationale, mandatory, critical,
        )?;
        if prediction.requires_unsupported_capability() {
            return Err(unsupported_mandatory_failure_class(EvolutionOperation::AdmitManifest));
        }
        Ok(prediction)
    }

    pub(crate) fn from_exact_parts(
        subject: PredictionSubject,
        metric: PredictionMetric,
        direction: PredictionDirection,
        threshold: MetricValue,
        rationale: BoundedText,
        mandatory: bool,
        critical: bool,
    ) -> Result<Self, EvolutionError> {
        if !threshold.compatible(metric)
            || matches!(metric, PredictionMetric::TaskPassAtK(_))
                != matches!(subject, PredictionSubject::Task(_))
            || matches!(metric, PredictionMetric::TaskPassAtK(0))
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "prediction subject, metric, and threshold are incompatible",
            ));
        }
        let bytes = prediction_bytes(
            subject, metric, direction, threshold, &rationale, mandatory, critical,
        );
        let digest = digest_parts(b"peritus.f0.prediction.v1\0", &[&bytes]);
        Ok(Self { subject, metric, direction, threshold, rationale, mandatory, critical, digest })
    }

    pub(crate) const fn requires_unsupported_capability(&self) -> bool {
        self.mandatory && matches!(self.subject, PredictionSubject::FailureClass(_))
    }

    /// Returns the prediction scope.
    #[must_use]
    pub const fn subject(&self) -> PredictionSubject {
        self.subject
    }
    /// Returns the observed metric.
    #[must_use]
    pub const fn metric(&self) -> PredictionMetric {
        self.metric
    }
    /// Returns the expected relation.
    #[must_use]
    pub const fn direction(&self) -> PredictionDirection {
        self.direction
    }
    /// Returns the declared threshold.
    #[must_use]
    pub const fn threshold(&self) -> MetricValue {
        self.threshold
    }
    /// Borrows the falsification rationale.
    #[must_use]
    pub const fn rationale(&self) -> &BoundedText {
        &self.rationale
    }
    /// Returns whether unavailability or contradiction denies eligibility.
    #[must_use]
    pub const fn mandatory(&self) -> bool {
        self.mandatory
    }
    /// Returns whether contradiction is a critical regression.
    #[must_use]
    pub const fn critical(&self) -> bool {
        self.critical
    }
    /// Returns the canonical prediction digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

pub(crate) const fn unsupported_mandatory_failure_class(
    operation: EvolutionOperation,
) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::UnsupportedCapability,
        operation,
        EvolutionRecovery::CorrectInput,
        "mandatory failure-class predictions require unavailable class-specific E3 evidence; use an observable campaign or task prediction",
    )
}

fn prediction_bytes(
    subject: PredictionSubject,
    metric: PredictionMetric,
    direction: PredictionDirection,
    threshold: MetricValue,
    rationale: &BoundedText,
    mandatory: bool,
    critical: bool,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(128 + rationale.as_str().len());
    match subject {
        PredictionSubject::Campaign => bytes.push(1),
        PredictionSubject::Task(id) => {
            bytes.push(2);
            bytes.extend_from_slice(id.as_bytes());
        }
        PredictionSubject::FailureClass(digest) => {
            bytes.push(3);
            bytes.extend_from_slice(digest.as_bytes());
        }
    }
    bytes.push(metric.tag());
    if let PredictionMetric::TaskPassAtK(k) = metric {
        bytes.extend_from_slice(&k.to_be_bytes());
    }
    bytes.push(match direction {
        PredictionDirection::AtLeast => 1,
        PredictionDirection::AtMost => 2,
        PredictionDirection::Equal => 3,
    });
    match threshold {
        MetricValue::SignedMillionths(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        MetricValue::ProbabilityMillionths(value) | MetricValue::Count(value) => {
            bytes.push(if matches!(threshold, MetricValue::Count(_)) { 3 } else { 2 });
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        MetricValue::Quantity(value) => {
            bytes.push(4);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    bytes.push(u8::from(mandatory));
    bytes.push(u8::from(critical));
    bytes.extend_from_slice(rationale.as_str().as_bytes());
    bytes
}
