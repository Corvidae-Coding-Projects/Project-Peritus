//! Independent criterion results and stable objective vectors.

use crate::{AttributionId, MetricValue, VariantId, identity::digest_parts};
use peritus_types::Sha256Digest;

/// Closed mandatory selection criterion catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Criterion {
    /// Paired-effect lower bound meets policy.
    PairedCorrectness,
    /// Critical regressions remain within policy.
    CriticalRegressions,
    /// Safety failures remain within policy.
    Safety,
    /// Evaluated-rollout reliability meets policy.
    Reliability,
    /// Attribution coverage meets policy.
    AttributionCoverage,
    /// Every mandatory prediction was confirmed.
    MandatoryPredictions,
    /// Candidate p95 latency remains within policy.
    Latency,
    /// Candidate mean provider cost remains within policy.
    Cost,
    /// Candidate mean input tokens remain within policy.
    InputTokens,
    /// Candidate mean output tokens remain within policy.
    OutputTokens,
    /// Required trace observations are complete.
    TraceCompleteness,
    /// Required teardown observations are complete.
    TeardownCompleteness,
    /// Required independent D2 review is complete.
    IndependentReview,
    /// Component/schema compatibility permits promotion.
    Compatibility,
}

impl Criterion {
    pub(crate) const fn tag(self) -> u8 {
        self as u8
    }
}

/// Independent criterion outcome; deny wins across the complete result set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CriterionOutcome {
    /// Mandatory policy was satisfied.
    Passed,
    /// Available evidence violated mandatory policy.
    Failed,
    /// Mandatory evidence was missing or unavailable.
    Unavailable,
}

/// One independently visible policy result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CriterionResult {
    criterion: Criterion,
    outcome: CriterionOutcome,
    observed: Option<MetricValue>,
    evidence_digest: Sha256Digest,
}

impl CriterionResult {
    pub(crate) const fn new(
        criterion: Criterion,
        outcome: CriterionOutcome,
        observed: Option<MetricValue>,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { criterion, outcome, observed, evidence_digest }
    }
    /// Returns the criterion identity.
    #[must_use]
    pub const fn criterion(self) -> Criterion {
        self.criterion
    }
    /// Returns pass, fail, or unavailable.
    #[must_use]
    pub const fn outcome(self) -> CriterionOutcome {
        self.outcome
    }
    /// Returns the exact observed metric when representable.
    #[must_use]
    pub const fn observed(self) -> Option<MetricValue> {
        self.observed
    }
    /// Returns the immutable evidence supporting the result.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}

/// Stable values compared only after all mandatory criteria pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectiveVector {
    pub(crate) paired_lower: i32,
    pub(crate) critical_regressions: u32,
    pub(crate) safety_failures: u32,
    pub(crate) reliability_lower: u32,
    pub(crate) latency_p95: u64,
    pub(crate) cost_mean: u64,
    pub(crate) input_tokens_mean: u64,
    pub(crate) output_tokens_mean: u64,
    pub(crate) attribution_coverage: u32,
}

impl ObjectiveVector {
    /// Returns the paired correctness lower bound.
    #[must_use]
    pub const fn paired_lower(self) -> i32 {
        self.paired_lower
    }
    /// Returns critical regression count.
    #[must_use]
    pub const fn critical_regressions(self) -> u32 {
        self.critical_regressions
    }
    /// Returns safety failure count.
    #[must_use]
    pub const fn safety_failures(self) -> u32 {
        self.safety_failures
    }
    /// Returns reliability lower bound.
    #[must_use]
    pub const fn reliability_lower(self) -> u32 {
        self.reliability_lower
    }
    /// Returns candidate p95 latency.
    #[must_use]
    pub const fn latency_p95(self) -> u64 {
        self.latency_p95
    }
    /// Returns candidate mean cost.
    #[must_use]
    pub const fn cost_mean(self) -> u64 {
        self.cost_mean
    }
    /// Returns candidate mean input tokens.
    #[must_use]
    pub const fn input_tokens_mean(self) -> u64 {
        self.input_tokens_mean
    }
    /// Returns candidate mean output tokens.
    #[must_use]
    pub const fn output_tokens_mean(self) -> u64 {
        self.output_tokens_mean
    }
    /// Returns attribution coverage millionths.
    #[must_use]
    pub const fn attribution_coverage(self) -> u32 {
        self.attribution_coverage
    }
}

/// Complete immutable eligibility assessment for one exact variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantAssessment {
    variant_id: VariantId,
    attribution_id: AttributionId,
    evidence_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    criteria: Vec<CriterionResult>,
    objectives: ObjectiveVector,
    digest: Sha256Digest,
}

impl VariantAssessment {
    pub(crate) fn from_exact_parts(
        variant_id: VariantId,
        attribution_id: AttributionId,
        evidence_digest: Sha256Digest,
        policy_digest: Sha256Digest,
        criteria: Vec<CriterionResult>,
        objectives: ObjectiveVector,
    ) -> Self {
        let digest = assessment_digest(
            variant_id,
            attribution_id,
            evidence_digest,
            policy_digest,
            &criteria,
            objectives,
        );
        Self {
            variant_id,
            attribution_id,
            evidence_digest,
            policy_digest,
            criteria,
            objectives,
            digest,
        }
    }
    /// Returns the assessed variant identity.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Returns the exact attribution identity.
    #[must_use]
    pub const fn attribution_id(&self) -> AttributionId {
        self.attribution_id
    }
    /// Returns the published evaluation evidence digest.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
    /// Returns the exact promotion policy used to produce this assessment.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Borrows every mandatory criterion in canonical order.
    #[must_use]
    pub fn criteria(&self) -> &[CriterionResult] {
        &self.criteria
    }
    /// Returns the stable objective vector.
    #[must_use]
    pub const fn objectives(&self) -> ObjectiveVector {
        self.objectives
    }
    /// Returns whether every mandatory criterion passed.
    #[must_use]
    pub fn eligible(&self) -> bool {
        self.criteria.iter().all(|result| result.outcome() == CriterionOutcome::Passed)
    }
    /// Returns the complete assessment digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn assessment_digest(
    variant_id: VariantId,
    attribution_id: AttributionId,
    evidence_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    criteria: &[CriterionResult],
    objectives: ObjectiveVector,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(variant_id.as_bytes());
    bytes.extend_from_slice(attribution_id.as_bytes());
    bytes.extend_from_slice(evidence_digest.as_bytes());
    bytes.extend_from_slice(policy_digest.as_bytes());
    for result in criteria {
        bytes.push(result.criterion().tag());
        bytes.push(match result.outcome() {
            CriterionOutcome::Passed => 1,
            CriterionOutcome::Failed => 2,
            CriterionOutcome::Unavailable => 3,
        });
        match result.observed() {
            None => bytes.push(0),
            Some(MetricValue::SignedMillionths(value)) => {
                bytes.extend_from_slice(&[1, 1]);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            Some(MetricValue::ProbabilityMillionths(value)) => {
                bytes.extend_from_slice(&[1, 2]);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            Some(MetricValue::Count(value)) => {
                bytes.extend_from_slice(&[1, 3]);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            Some(MetricValue::Quantity(value)) => {
                bytes.extend_from_slice(&[1, 4]);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes.extend_from_slice(result.evidence_digest().as_bytes());
    }
    bytes.extend_from_slice(&objectives.paired_lower.to_be_bytes());
    bytes.extend_from_slice(&objectives.critical_regressions.to_be_bytes());
    bytes.extend_from_slice(&objectives.safety_failures.to_be_bytes());
    bytes.extend_from_slice(&objectives.reliability_lower.to_be_bytes());
    bytes.extend_from_slice(&objectives.latency_p95.to_be_bytes());
    bytes.extend_from_slice(&objectives.cost_mean.to_be_bytes());
    bytes.extend_from_slice(&objectives.input_tokens_mean.to_be_bytes());
    bytes.extend_from_slice(&objectives.output_tokens_mean.to_be_bytes());
    bytes.extend_from_slice(&objectives.attribution_coverage.to_be_bytes());
    digest_parts(b"peritus.f0.variant-assessment.v1\0", &[&bytes])
}
