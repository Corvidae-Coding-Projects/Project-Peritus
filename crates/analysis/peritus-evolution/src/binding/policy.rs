//! Typed immutable promotion policy and protected E1 binding.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
    identity::{digest_parts, push_bytes},
};
use peritus_harness::domain::{
    ComponentId, ComponentKind, HarnessRevision, HarnessRevisionIdentity, ProtectionClass,
};
use peritus_types::Sha256Digest;

/// Resource measurement that may constrain eligibility or order eligible variants.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PromotionMeasurement {
    /// Candidate p95 end-to-end latency.
    LatencyP95,
    /// Candidate mean provider cost.
    CostMean,
    /// Candidate mean provider input tokens.
    InputTokensMean,
    /// Candidate mean provider output tokens.
    OutputTokensMean,
}

impl PromotionMeasurement {
    const fn bit(self) -> u8 {
        match self {
            Self::LatencyP95 => 1,
            Self::CostMean => 1 << 1,
            Self::InputTokensMean => 1 << 2,
            Self::OutputTokensMean => 1 << 3,
        }
    }
}

/// Evaluator resource measurements known to be producible before a campaign is frozen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasurementCapabilities(u8);

impl MeasurementCapabilities {
    /// No resource measurement capabilities.
    pub const NONE: Self = Self(0);
    /// Every resource measurement capability understood by F0.
    pub const ALL: Self = Self(0b1111);

    /// Constructs an explicit evaluator capability set.
    #[must_use]
    pub const fn new(
        latency_p95: bool,
        cost_mean: bool,
        input_tokens_mean: bool,
        output_tokens_mean: bool,
    ) -> Self {
        Self(
            (latency_p95 as u8)
                | ((cost_mean as u8) << 1)
                | ((input_tokens_mean as u8) << 2)
                | ((output_tokens_mean as u8) << 3),
        )
    }

    /// Returns whether the evaluator can produce one measurement.
    #[must_use]
    pub const fn includes(self, measurement: PromotionMeasurement) -> bool {
        self.0 & measurement.bit() != 0
    }

    /// Returns whether every required capability is present.
    #[must_use]
    pub const fn satisfies(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub(crate) const fn bits(self) -> u8 {
        self.0
    }

    pub(crate) const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !Self::ALL.0 == 0 { Some(Self(bits)) } else { None }
    }

    const fn with(self, measurement: PromotionMeasurement) -> Self {
        Self(self.0 | measurement.bit())
    }
}

/// Whether one resource measurement is optional or an explicit eligibility constraint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeasurementRequirement {
    /// The measurement may remain unavailable unless selected as a ranking objective.
    Optional,
    /// The measurement is mandatory and must not exceed this independent maximum.
    RequiredMaximum(u64),
}

impl MeasurementRequirement {
    /// Returns the explicit maximum when this is a mandatory measurement.
    #[must_use]
    pub const fn maximum(self) -> Option<u64> {
        match self {
            Self::Optional => None,
            Self::RequiredMaximum(value) => Some(value),
        }
    }

    /// Returns whether this measurement independently gates eligibility.
    #[must_use]
    pub const fn required(self) -> bool {
        matches!(self, Self::RequiredMaximum(_))
    }
}

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
    latency_p95: MeasurementRequirement,
    cost_mean: MeasurementRequirement,
    input_tokens_mean: MeasurementRequirement,
    output_tokens_mean: MeasurementRequirement,
    require_complete_trace: bool,
    require_complete_teardown: bool,
    explicit_measurements: bool,
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
        Self::new_inner(
            minimum_paired_lower_millionths,
            maximum_critical_regressions,
            maximum_safety_failures,
            minimum_reliability_lower_millionths,
            minimum_attribution_coverage_millionths,
            MeasurementRequirement::RequiredMaximum(maximum_latency_p95_micros),
            MeasurementRequirement::RequiredMaximum(maximum_cost_mean_microunits),
            MeasurementRequirement::RequiredMaximum(maximum_input_tokens_mean),
            MeasurementRequirement::RequiredMaximum(maximum_output_tokens_mean),
            require_complete_trace,
            require_complete_teardown,
            false,
        )
    }

    /// Constructs thresholds with explicit required or optional resource measurements.
    ///
    /// Ranking objectives remain separate and make their corresponding measurement required even
    /// when the independent threshold is optional.
    ///
    /// # Errors
    /// Rejects millionths outside their representable probability/effect domains.
    #[allow(clippy::too_many_arguments, reason = "independent deny-wins thresholds stay explicit")]
    pub const fn new_with_measurements(
        minimum_paired_lower_millionths: i32,
        maximum_critical_regressions: u32,
        maximum_safety_failures: u32,
        minimum_reliability_lower_millionths: u32,
        minimum_attribution_coverage_millionths: u32,
        latency_p95: MeasurementRequirement,
        cost_mean: MeasurementRequirement,
        input_tokens_mean: MeasurementRequirement,
        output_tokens_mean: MeasurementRequirement,
        require_complete_trace: bool,
        require_complete_teardown: bool,
    ) -> Result<Self, EvolutionError> {
        Self::new_inner(
            minimum_paired_lower_millionths,
            maximum_critical_regressions,
            maximum_safety_failures,
            minimum_reliability_lower_millionths,
            minimum_attribution_coverage_millionths,
            latency_p95,
            cost_mean,
            input_tokens_mean,
            output_tokens_mean,
            require_complete_trace,
            require_complete_teardown,
            true,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "independent deny-wins thresholds stay explicit")]
    const fn new_inner(
        minimum_paired_lower_millionths: i32,
        maximum_critical_regressions: u32,
        maximum_safety_failures: u32,
        minimum_reliability_lower_millionths: u32,
        minimum_attribution_coverage_millionths: u32,
        latency_p95: MeasurementRequirement,
        cost_mean: MeasurementRequirement,
        input_tokens_mean: MeasurementRequirement,
        output_tokens_mean: MeasurementRequirement,
        require_complete_trace: bool,
        require_complete_teardown: bool,
        explicit_measurements: bool,
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
            latency_p95,
            cost_mean,
            input_tokens_mean,
            output_tokens_mean,
            require_complete_trace,
            require_complete_teardown,
            explicit_measurements,
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
    /// Required maximum p95 end-to-end latency, or `None` when optional.
    #[must_use]
    pub const fn maximum_latency_p95_micros(self) -> Option<u64> {
        self.latency_p95.maximum()
    }
    /// Required maximum mean provider cost, or `None` when optional.
    #[must_use]
    pub const fn maximum_cost_mean_microunits(self) -> Option<u64> {
        self.cost_mean.maximum()
    }
    /// Required maximum mean provider input tokens, or `None` when optional.
    #[must_use]
    pub const fn maximum_input_tokens_mean(self) -> Option<u64> {
        self.input_tokens_mean.maximum()
    }
    /// Required maximum mean provider output tokens, or `None` when optional.
    #[must_use]
    pub const fn maximum_output_tokens_mean(self) -> Option<u64> {
        self.output_tokens_mean.maximum()
    }
    /// Returns the explicit policy for one resource measurement.
    #[must_use]
    pub const fn measurement(self, measurement: PromotionMeasurement) -> MeasurementRequirement {
        match measurement {
            PromotionMeasurement::LatencyP95 => self.latency_p95,
            PromotionMeasurement::CostMean => self.cost_mean,
            PromotionMeasurement::InputTokensMean => self.input_tokens_mean,
            PromotionMeasurement::OutputTokensMean => self.output_tokens_mean,
        }
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

    pub(crate) const fn has_explicit_measurements(self) -> bool {
        self.explicit_measurements
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
            || limits
                .criteria_limit()
                .is_some_and(|maximum| objectives.len() > usize::from(maximum))
            || objectives
                .iter()
                .enumerate()
                .any(|(index, value)| objectives[index + 1..].contains(value))
            || review_required_kinds.windows(2).any(|pair| pair[0] >= pair[1])
            || maximum_variants == 0
            || limits.variants_limit().is_some_and(|limit| maximum_variants > limit)
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
    /// Returns resource measurements required by either an eligibility threshold or ranking.
    #[must_use]
    pub fn required_measurements(&self) -> MeasurementCapabilities {
        let mut required = MeasurementCapabilities::NONE;
        for measurement in [
            PromotionMeasurement::LatencyP95,
            PromotionMeasurement::CostMean,
            PromotionMeasurement::InputTokensMean,
            PromotionMeasurement::OutputTokensMean,
        ] {
            if self.thresholds.measurement(measurement).required()
                || self.objectives.iter().any(|objective| {
                    objective_measurement(*objective) == Some(measurement)
                })
            {
                required = required.with(measurement);
            }
        }
        required
    }
    /// Returns whether resource measurement roles were declared explicitly.
    #[must_use]
    pub const fn has_explicit_measurements(&self) -> bool {
        self.thresholds.has_explicit_measurements()
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
    if thresholds.has_explicit_measurements() {
        return policy_digest_v2(
            thresholds,
            objectives,
            review_kinds,
            allow_cross_lineage,
            maximum_variants,
        );
    }
    let mut bytes = Vec::with_capacity(160);
    bytes.extend_from_slice(&thresholds.minimum_paired_lower_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_critical_regressions.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_safety_failures.to_be_bytes());
    bytes.extend_from_slice(&thresholds.minimum_reliability_lower_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.minimum_attribution_coverage_millionths.to_be_bytes());
    for requirement in [
        thresholds.latency_p95,
        thresholds.cost_mean,
        thresholds.input_tokens_mean,
        thresholds.output_tokens_mean,
    ] {
        let MeasurementRequirement::RequiredMaximum(maximum) = requirement else {
            unreachable!("legacy promotion policy has an optional measurement")
        };
        bytes.extend_from_slice(&maximum.to_be_bytes());
    }
    bytes.push(u8::from(thresholds.require_complete_trace));
    bytes.push(u8::from(thresholds.require_complete_teardown));
    push_bytes(&mut bytes, &objectives.iter().map(|value| value.tag()).collect::<Vec<_>>());
    push_bytes(&mut bytes, &review_kinds.iter().map(|value| value.tag()).collect::<Vec<_>>());
    bytes.push(u8::from(allow_cross_lineage));
    bytes.extend_from_slice(&maximum_variants.to_be_bytes());
    digest_parts(b"peritus.f0.promotion-policy.v1\0", &[&bytes])
}

fn policy_digest_v2(
    thresholds: PromotionThresholds,
    objectives: &[Objective],
    review_kinds: &[ComponentKind],
    allow_cross_lineage: bool,
    maximum_variants: u16,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(168);
    bytes.extend_from_slice(&thresholds.minimum_paired_lower_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_critical_regressions.to_be_bytes());
    bytes.extend_from_slice(&thresholds.maximum_safety_failures.to_be_bytes());
    bytes.extend_from_slice(&thresholds.minimum_reliability_lower_millionths.to_be_bytes());
    bytes.extend_from_slice(&thresholds.minimum_attribution_coverage_millionths.to_be_bytes());
    for requirement in [
        thresholds.latency_p95,
        thresholds.cost_mean,
        thresholds.input_tokens_mean,
        thresholds.output_tokens_mean,
    ] {
        match requirement {
            MeasurementRequirement::Optional => bytes.push(0),
            MeasurementRequirement::RequiredMaximum(maximum) => {
                bytes.push(1);
                bytes.extend_from_slice(&maximum.to_be_bytes());
            }
        }
    }
    bytes.push(u8::from(thresholds.require_complete_trace));
    bytes.push(u8::from(thresholds.require_complete_teardown));
    push_bytes(&mut bytes, &objectives.iter().map(|value| value.tag()).collect::<Vec<_>>());
    push_bytes(&mut bytes, &review_kinds.iter().map(|value| value.tag()).collect::<Vec<_>>());
    bytes.push(u8::from(allow_cross_lineage));
    bytes.extend_from_slice(&maximum_variants.to_be_bytes());
    digest_parts(b"peritus.f0.promotion-policy.v2\0", &[&bytes])
}

const fn objective_measurement(objective: Objective) -> Option<PromotionMeasurement> {
    match objective {
        Objective::Latency => Some(PromotionMeasurement::LatencyP95),
        Objective::Cost => Some(PromotionMeasurement::CostMean),
        Objective::InputTokens => Some(PromotionMeasurement::InputTokensMean),
        Objective::OutputTokens => Some(PromotionMeasurement::OutputTokensMean),
        Objective::PairedCorrectness
        | Objective::CriticalRegressions
        | Objective::SafetyFailures
        | Objective::Reliability
        | Objective::AttributionCoverage => None,
    }
}

const fn mismatch() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::BindPolicy,
        EvolutionRecovery::CorrectInput,
        "typed policy differs from the protected E1 evolution strategy",
    )
}
