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
    stop_after_attempt: Option<u16>,
    initial_backoff_micros: u64,
    maximum_backoff_micros: u64,
}

impl EvaluationRetryPolicy {
    /// Creates a checked exact retry policy.
    ///
    /// # Errors
    /// Rejects zero attempts or reversed backoff bounds. The attempt-page size is a storage
    /// concern and does not change the caller's stopping policy.
    pub const fn new(
        maximum_attempts: u16,
        initial_backoff_micros: u64,
        maximum_backoff_micros: u64,
        _limits: EvaluationLimits,
    ) -> Result<Self, EvaluationError> {
        if maximum_attempts == 0 || initial_backoff_micros > maximum_backoff_micros {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "evaluation retry policy is invalid",
            ));
        }
        Ok(Self {
            stop_after_attempt: Some(maximum_attempts),
            initial_backoff_micros,
            maximum_backoff_micros,
        })
    }
    /// Creates a persistent retry policy with no synthetic attempt stopping point.
    ///
    /// # Errors
    /// Rejects reversed backoff bounds.
    pub const fn persistent(
        initial_backoff_micros: u64,
        maximum_backoff_micros: u64,
    ) -> Result<Self, EvaluationError> {
        if initial_backoff_micros > maximum_backoff_micros {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "evaluation retry policy is invalid",
            ));
        }
        Ok(Self {
            stop_after_attempt: None,
            initial_backoff_micros,
            maximum_backoff_micros,
        })
    }
    /// Returns the caller-selected stopping attempt, or `None` for persistent operation.
    #[must_use]
    pub const fn stop_after_attempt(self) -> Option<u16> {
        self.stop_after_attempt
    }
    /// Returns whether retries remain persistent until the caller changes policy or cancels.
    #[must_use]
    pub const fn is_persistent(self) -> bool {
        self.stop_after_attempt.is_none()
    }
    /// Returns the finite stopping attempt, or the largest representable attempt for adapters
    /// whose legacy interface cannot express persistence.
    #[must_use]
    pub const fn maximum_attempts(self) -> u16 {
        match self.stop_after_attempt {
            Some(value) => value,
            None => u16::MAX,
        }
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

    pub(crate) const fn canonical_stop_after_attempt(self) -> u16 {
        match self.stop_after_attempt {
            Some(value) => value,
            None => 0,
        }
    }

    pub(crate) const fn validate(self) -> Result<(), EvaluationError> {
        if matches!(self.stop_after_attempt, Some(0))
            || self.initial_backoff_micros > self.maximum_backoff_micros
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "evaluation retry policy is invalid",
            ));
        }
        Ok(())
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
        let policy = Self {
            pass_k,
            bootstrap_replicates,
            confidence_millionths,
            instability_threshold_millionths,
            require_complete_usage,
        };
        policy.validate_against(limits)?;
        Ok(policy)
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

    pub(crate) fn validate_against(
        &self,
        limits: EvaluationLimits,
    ) -> Result<(), EvaluationError> {
        if self.pass_k.is_empty()
            || self.pass_k.len() > usize::from(limits.pass_k_values())
            || self.pass_k.windows(2).any(|pair| pair[0] >= pair[1])
            || self.pass_k[0] == 0
            || self.bootstrap_replicates == 0
            || self.bootstrap_replicates > limits.bootstrap_replicates()
            || self.confidence_millionths != 950_000
            || self.instability_threshold_millionths > 1_000_000
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "metric policy is noncanonical or exceeds supported bounds",
            ));
        }
        Ok(())
    }
}
