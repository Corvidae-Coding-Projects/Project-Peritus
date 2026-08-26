//! Typed immutable promotion policy and protected E1 binding.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
    identity::{digest_parts, push_bytes},
};
use peritus_harness::domain::{
    ComponentId, ComponentKind, HarnessRevision, HarnessRevisionIdentity, ProtectionClass,
};
use peritus_types::Sha256Digest;

/// Stable objective values used after mandatory deny-wins eligibility.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Objective {
    /// Prefer the higher paired-effect lower confidence bound.
    PairedCorrectness,
    /// Prefer fewer critical task regressions.
    CriticalRegressions,
    /// Prefer fewer safety failures.
    SafetyFailures,
    /// Prefer the higher evaluated-rollout reliability lower bound.
    Reliability,
    /// Prefer lower p95 end-to-end latency.
    Latency,
    /// Prefer lower mean provider cost.
    Cost,
    /// Prefer fewer mean provider input tokens.
    InputTokens,
    /// Prefer fewer mean provider output tokens.
    OutputTokens,
    /// Prefer higher falsification coverage.
    AttributionCoverage,
}

impl Objective {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::PairedCorrectness => 1,
            Self::CriticalRegressions => 2,
            Self::SafetyFailures => 3,
            Self::Reliability => 4,
            Self::Latency => 5,
            Self::Cost => 6,
            Self::InputTokens => 7,
            Self::OutputTokens => 8,
            Self::AttributionCoverage => 9,
        }
    }
}

/// Independent hard thresholds that determine promotion eligibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionThresholds {
    minimum_paired_lower_millionths: i32,
    maximum_critical_regressions: u32,
    maximum_safety_failures: u32,
    minimum_reliability_lower_millionths: u32,
    minimum_attribution_coverage_millionths: u32,
    maximum_latency_p95_micros: u64,
    maximum_cost_mean_microunits: u64,
    maximum_input_tokens_mean: u64,
    maximum_output_tokens_mean: u64,
    require_complete_trace: bool,
    require_complete_teardown: bool,
}

impl PromotionThresholds {
    /// Constructs checked independent promotion thresholds.
    ///
    /// # Errors
    /// Rejects millionths outside their representable probability/effect domains.
    #[allow(clippy::too_many_arguments, reason = "independent deny-wins thresholds stay explicit")]
    pub const fn new(
        minimum_paired_lower_millionths: i32,
        maximum_critical_regressions: u32,
        maximum_safety_failures: u32,
        minimum_reliability_lower_millionths: u32,
        minimum_attribution_coverage_millionths: u32,
        maximum_latency_p95_micros: u64,
        maximum_cost_mean_microunits: u64,
        maximum_input_tokens_mean: u64,
        maximum_output_tokens_mean: u64,
        require_complete_trace: bool,
        require_complete_teardown: bool,
    ) -> Result<Self, EvolutionError> {
        if minimum_paired_lower_millionths < -1_000_000
            || minimum_paired_lower_millionths > 1_000_000
            || minimum_reliability_lower_millionths > 1_000_000
            || minimum_attribution_coverage_millionths > 1_000_000
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::BindPolicy,
                EvolutionRecovery::CorrectInput,
                "promotion threshold millionths are outside their domain",
            ));
        }
        Ok(Self {
            minimum_paired_lower_millionths,
            maximum_critical_regressions,
            maximum_safety_failures,
            minimum_reliability_lower_millionths,
            minimum_attribution_coverage_millionths,
            maximum_latency_p95_micros,
            maximum_cost_mean_microunits,
            maximum_input_tokens_mean,
            maximum_output_tokens_mean,
            require_complete_trace,
            require_complete_teardown,
        })
    }

    /// Minimum accepted paired-effect lower bound.
    #[must_use]
    pub const fn minimum_paired_lower_millionths(self) -> i32 {
        self.minimum_paired_lower_millionths
    }
    /// Maximum critical regressions.
    #[must_use]
    pub const fn maximum_critical_regressions(self) -> u32 {
        self.maximum_critical_regressions
    }
    /// Maximum valid evaluator safety failures.
    #[must_use]
    pub const fn maximum_safety_failures(self) -> u32 {
        self.maximum_safety_failures
    }
    /// Minimum evaluated-rollout reliability lower bound.
    #[must_use]
    pub const fn minimum_reliability_lower_millionths(self) -> u32 {
        self.minimum_reliability_lower_millionths
    }
    /// Minimum decidable prediction coverage.
    #[must_use]
    pub const fn minimum_attribution_coverage_millionths(self) -> u32 {
        self.minimum_attribution_coverage_millionths
    }
    /// Maximum p95 end-to-end latency.
    #[must_use]
    pub const fn maximum_latency_p95_micros(self) -> u64 {
        self.maximum_latency_p95_micros
    }
    /// Maximum mean provider cost.
    #[must_use]
    pub const fn maximum_cost_mean_microunits(self) -> u64 {
        self.maximum_cost_mean_microunits
    }
    /// Maximum mean provider input tokens.
    #[must_use]
    pub const fn maximum_input_tokens_mean(self) -> u64 {
        self.maximum_input_tokens_mean
    }
    /// Maximum mean provider output tokens.
    #[must_use]
    pub const fn maximum_output_tokens_mean(self) -> u64 {
        self.maximum_output_tokens_mean
    }
    /// Whether every rollout must have a complete trace.
    #[must_use]
    pub const fn require_complete_trace(self) -> bool {
        self.require_complete_trace
    }
    /// Whether every rollout must have complete teardown.
    #[must_use]
    pub const fn require_complete_teardown(self) -> bool {
        self.require_complete_teardown
    }
}

/// Immutable schema-v1 selection, review, and compatibility policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionPolicy {
    thresholds: PromotionThresholds,
    objectives: Vec<Objective>,
    review_required_kinds: Vec<ComponentKind>,
    allow_cross_lineage: bool,
    maximum_variants: u16,
    digest: Sha256Digest,
}

impl PromotionPolicy {
    /// Constructs one canonical bounded schema-v1 policy.
    ///
    /// # Errors
    /// Rejects empty, duplicated, noncanonical, or over-limit objective/review collections.
    pub fn new(
        thresholds: PromotionThresholds,
        objectives: Vec<Objective>,
        review_required_kinds: Vec<ComponentKind>,
        allow_cross_lineage: bool,
        maximum_variants: u16,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if objectives.is_empty()
            || objectives.len() > usize::from(limits.criteria())
            || objectives
                .iter()
                .enumerate()
                .any(|(index, value)| objectives[index + 1..].contains(value))
            || review_required_kinds.windows(2).any(|pair| pair[0] >= pair[1])
            || maximum_variants == 0
            || maximum_variants > limits.variants()
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::NonCanonical,
                EvolutionOperation::BindPolicy,
                EvolutionRecovery::CorrectInput,
                "promotion policy collections or variant bound are invalid",
            ));
        }
        let digest = policy_digest(
            thresholds,
            &objectives,
            &review_required_kinds,
            allow_cross_lineage,
            maximum_variants,
        );
        Ok(Self {
            thresholds,
            objectives,
            review_required_kinds,
            allow_cross_lineage,
            maximum_variants,
            digest,
        })
    }

    /// Returns all independent mandatory thresholds.
    #[must_use]
    pub const fn thresholds(&self) -> PromotionThresholds {
        self.thresholds
    }
    /// Borrows the stable lexicographic objective order.
    #[must_use]
    pub fn objectives(&self) -> &[Objective] {
        &self.objectives
    }
    /// Borrows component kinds requiring completed D2 review.
    #[must_use]
    pub fn review_required_kinds(&self) -> &[ComponentKind] {
        &self.review_required_kinds
    }
    /// Returns whether a candidate may cross harness lineage.
    #[must_use]
    pub const fn allow_cross_lineage(&self) -> bool {
        self.allow_cross_lineage
    }
    /// Returns the maximum campaign variant population.
    #[must_use]
    pub const fn maximum_variants(&self) -> u16 {
        self.maximum_variants
    }
    /// Returns the canonical policy digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Typed policy tied to the protected E1 `EvolutionStrategy` declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionPolicyBinding {
    production_revision: HarnessRevisionIdentity,
    component_id: ComponentId,
    component_digest: Sha256Digest,
    policy: PromotionPolicy,
    digest: Sha256Digest,
}

impl PromotionPolicyBinding {
    /// Captures a typed policy whose canonical bytes are the protected E1 component content.
    ///
    /// # Errors
    /// Rejects absent, wrong-kind, unprotected, or digest-mismatched declarations.
    pub fn capture(
        production: &HarnessRevision,
        component_id: &ComponentId,
        policy: PromotionPolicy,
    ) -> Result<Self, EvolutionError> {
        let declaration = production.graph().declaration(component_id).ok_or_else(mismatch)?;
        if declaration.kind() != ComponentKind::EvolutionStrategy
            || declaration.protection_class() != ProtectionClass::ProductionPromotion
            || declaration.content_digest() != policy.digest()
        {
            return Err(mismatch());
        }
        let digest = digest_parts(
            b"peritus.f0.promotion-policy-binding.v1\0",
            &[
                production.harness_id().as_bytes(),
                production.digest().as_bytes(),
                component_id.as_str().as_bytes(),
                declaration.content_digest().as_bytes(),
                policy.digest().as_bytes(),
            ],
        );
        Ok(Self {
            production_revision: production.identity(),
            component_id: component_id.clone(),
            component_digest: declaration.content_digest(),
            policy,
            digest,
        })
    }

    pub(crate) fn from_exact_parts(
        production_revision: HarnessRevisionIdentity,
        component_id: ComponentId,
        component_digest: Sha256Digest,
        policy: PromotionPolicy,
    ) -> Result<Self, EvolutionError> {
        if component_digest != policy.digest() {
            return Err(mismatch());
        }
        let digest = digest_parts(
            b"peritus.f0.promotion-policy-binding.v1\0",
            &[
                production_revision.harness_id().as_bytes(),
                production_revision.digest().as_bytes(),
                component_id.as_str().as_bytes(),
                component_digest.as_bytes(),
                policy.digest().as_bytes(),
            ],
        );
        Ok(Self { production_revision, component_id, component_digest, policy, digest })
    }

    /// Returns the owning production revision.
    #[must_use]
    pub const fn production_revision(&self) -> HarnessRevisionIdentity {
        self.production_revision
    }
    /// Returns the protected E1 declaration identity.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }
    /// Returns the exact protected component content digest.
    #[must_use]
    pub const fn component_digest(&self) -> Sha256Digest {
        self.component_digest
    }
    /// Borrows the typed immutable promotion policy.
    #[must_use]
    pub const fn policy(&self) -> &PromotionPolicy {
        &self.policy
    }
    /// Returns the complete policy-binding digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns whether a candidate preserves the exact protected policy declaration.
    #[must_use]
    pub fn preserved_by(&self, candidate: &HarnessRevision) -> bool {
        candidate.graph().declaration(&self.component_id).is_some_and(|declaration| {
            declaration.kind() == ComponentKind::EvolutionStrategy
                && declaration.protection_class() == ProtectionClass::ProductionPromotion
                && declaration.content_digest() == self.component_digest
        })
    }
}

fn policy_digest(
    thresholds: PromotionThresholds,
    objectives: &[Objective],
    review_kinds: &[ComponentKind],
    allow_cross_lineage: bool,
    maximum_variants: u16,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(160);
    bytes.extend_from_slice(&thresholds.minimum_paired_lower_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_critical_regressions.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_safety_failures.to_be_bytes());
    bytes.extend_from_slice(&thresholds.minimum_reliability_lower_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.minimum_attribution_coverage_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_latency_p95_micros.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_cost_mean_microunits.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_input_tokens_mean.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_output_tokens_mean.to_be_bytes());
    bytes.push(u8::from(thresholds.require_complete_trace));
    bytes.push(u8::from(thresholds.require_complete_teardown));
    push_bytes(&mut bytes, &objectives.iter().map(|value| value.tag()).collect::<Vec<_>>());
    push_bytes(&mut bytes, &review_kinds.iter().map(|value| value.tag()).collect::<Vec<_>>());
    bytes.push(u8::from(allow_cross_lineage));
    bytes.extend_from_slice(&maximum_variants.to_be_bytes());
    digest_parts(b"peritus.f0.promotion-policy.v1\0", &[&bytes])
}

const fn mismatch() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::BindPolicy,
        EvolutionRecovery::CorrectInput,
        "typed policy differs from the protected E1 evolution strategy",
    )
}
