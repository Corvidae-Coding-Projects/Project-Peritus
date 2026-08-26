//! Versioned retry, metric, seed, and infrastructure policies.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationLimits, EvaluationOperation};

/// Whether the provider receives the reproducible E3 seed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SeedDeliveryPolicy {
    /// The provider profile must support and receive deterministic sampling controls.
    Required,
    /// The seed is retained for pairing/order but the provider does not receive it.
    RecordedOnly,
}

impl SeedDeliveryPolicy {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Required => 1,
            Self::RecordedOnly => 2,
        }
    }
}

/// Frozen retry policy for one logical rollout.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvaluationRetryPolicy {
    maximum_attempts: u16,
    initial_backoff_micros: u64,
    maximum_backoff_micros: u64,
}

impl EvaluationRetryPolicy {
    /// Creates a checked exact retry policy.
    ///
    /// # Errors
    /// Rejects zero attempts, attempts above E3 limits, or reversed backoff bounds.
    pub const fn new(
        maximum_attempts: u16,
        initial_backoff_micros: u64,
        maximum_backoff_micros: u64,
        limits: EvaluationLimits,
    ) -> Result<Self, EvaluationError> {
        if maximum_attempts == 0
            || maximum_attempts > limits.attempts_per_rollout()
            || initial_backoff_micros > maximum_backoff_micros
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "evaluation retry policy is invalid",
            ));
        }
        Ok(Self { maximum_attempts, initial_backoff_micros, maximum_backoff_micros })
    }
    /// Returns the attempt ceiling.
    #[must_use]
    pub const fn maximum_attempts(self) -> u16 {
        self.maximum_attempts
    }
    /// Returns initial deterministic backoff.
    #[must_use]
    pub const fn initial_backoff_micros(self) -> u64 {
        self.initial_backoff_micros
    }
    /// Returns maximum deterministic backoff.
    #[must_use]
    pub const fn maximum_backoff_micros(self) -> u64 {
        self.maximum_backoff_micros
    }
}

/// Frozen treatment of infrastructure failures for one metric.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InfrastructureTreatment {
    /// Include infrastructure terminals as unsuccessful observations.
    CountAsFailure,
    /// Exclude them while retaining and reporting the excluded denominator.
    ExcludeWithDenominator,
    /// Make the affected metric unavailable.
    InvalidateMetric,
}

impl InfrastructureTreatment {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::CountAsFailure => 1,
            Self::ExcludeWithDenominator => 2,
            Self::InvalidateMetric => 3,
        }
    }
}

/// Complete frozen infrastructure accounting policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InfrastructurePolicy {
    correctness: InfrastructureTreatment,
    reliability: InfrastructureTreatment,
    resource: InfrastructureTreatment,
}

impl InfrastructurePolicy {
    /// Creates an explicit per-metric infrastructure policy.
    #[must_use]
    pub const fn new(
        correctness: InfrastructureTreatment,
        reliability: InfrastructureTreatment,
        resource: InfrastructureTreatment,
    ) -> Self {
        Self { correctness, reliability, resource }
    }
    /// Correctness treatment.
    #[must_use]
    pub const fn correctness(self) -> InfrastructureTreatment {
        self.correctness
    }
    /// Reliability treatment.
    #[must_use]
    pub const fn reliability(self) -> InfrastructureTreatment {
        self.reliability
    }
    /// Resource treatment.
    #[must_use]
    pub const fn resource(self) -> InfrastructureTreatment {
        self.resource
    }
}

/// Complete version-one statistical policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricPolicy {
    pass_k: Vec<u16>,
    bootstrap_replicates: u32,
    confidence_millionths: u32,
    instability_threshold_millionths: u32,
    require_complete_usage: bool,
}

impl MetricPolicy {
    /// Creates a canonical metric policy.
    ///
    /// # Errors
    /// Rejects empty/noncanonical k, unsupported confidence, or exceeded bounds.
    pub fn new(
        pass_k: Vec<u16>,
        bootstrap_replicates: u32,
        confidence_millionths: u32,
        instability_threshold_millionths: u32,
        require_complete_usage: bool,
        limits: EvaluationLimits,
    ) -> Result<Self, EvaluationError> {
        if pass_k.is_empty()
            || pass_k.len() > usize::from(limits.pass_k_values())
            || pass_k.windows(2).any(|pair| pair[0] >= pair[1])
            || pass_k[0] == 0
            || bootstrap_replicates == 0
            || bootstrap_replicates > limits.bootstrap_replicates()
            || confidence_millionths != 950_000
            || instability_threshold_millionths > 1_000_000
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "metric policy is noncanonical or exceeds supported bounds",
            ));
        }
        Ok(Self {
            pass_k,
            bootstrap_replicates,
            confidence_millionths,
            instability_threshold_millionths,
            require_complete_usage,
        })
    }
    /// Borrows ascending distinct pass@k values.
    #[must_use]
    pub fn pass_k(&self) -> &[u16] {
        &self.pass_k
    }
    /// Returns deterministic task-cluster bootstrap replicates.
    #[must_use]
    pub const fn bootstrap_replicates(&self) -> u32 {
        self.bootstrap_replicates
    }
    /// Returns frozen confidence in millionths.
    #[must_use]
    pub const fn confidence_millionths(&self) -> u32 {
        self.confidence_millionths
    }
    /// Returns mixed-outcome instability threshold.
    #[must_use]
    pub const fn instability_threshold_millionths(&self) -> u32 {
        self.instability_threshold_millionths
    }
    /// Returns whether every usage value is required.
    #[must_use]
    pub const fn require_complete_usage(&self) -> bool {
        self.require_complete_usage
    }
}
