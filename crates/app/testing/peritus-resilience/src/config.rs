//! Validated qualification and subject-observation bounds.

use std::error::Error;
use std::fmt;

/// Absolute maximum number of scenarios accepted by one runner invocation.
pub const HARD_MAX_SCENARIOS: u16 = 128;
/// Absolute maximum number of milestones returned for one scenario.
pub const HARD_MAX_MILESTONES: u16 = 64;
/// Absolute maximum configured retry count per retry class.
pub const HARD_MAX_RETRIES: u16 = 32;

/// Invalid qualification bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationError {
    field: &'static str,
    value: u64,
    minimum: u64,
    maximum: u64,
}

impl ConfigurationError {
    const fn new(field: &'static str, value: u64, minimum: u64, maximum: u64) -> Self {
        Self { field, value, minimum, maximum }
    }

    /// Returns the stable field name.
    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }

    /// Returns the rejected value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} must be in {}..={}; received {}",
            self.field, self.minimum, self.maximum, self.value
        )
    }
}

impl Error for ConfigurationError {}

/// Independent retry ceilings observed in every scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryLimits {
    provider: u16,
    tool: u16,
    worker: u16,
    reconciliation: u16,
}

impl RetryLimits {
    /// Creates nonzero retry ceilings bounded by [`HARD_MAX_RETRIES`].
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] when any ceiling is zero or exceeds
    /// [`HARD_MAX_RETRIES`].
    pub fn new(
        provider: u16,
        tool: u16,
        worker: u16,
        reconciliation: u16,
    ) -> Result<Self, ConfigurationError> {
        validate("provider_retries", provider, 1, HARD_MAX_RETRIES)?;
        validate("tool_retries", tool, 1, HARD_MAX_RETRIES)?;
        validate("worker_restarts", worker, 1, HARD_MAX_RETRIES)?;
        validate("reconciliation_steps", reconciliation, 1, HARD_MAX_RETRIES)?;
        Ok(Self { provider, tool, worker, reconciliation })
    }

    /// Returns the provider retry ceiling.
    #[must_use]
    pub const fn provider(self) -> u16 {
        self.provider
    }
    /// Returns the tool retry ceiling.
    #[must_use]
    pub const fn tool(self) -> u16 {
        self.tool
    }
    /// Returns the worker restart ceiling.
    #[must_use]
    pub const fn worker(self) -> u16 {
        self.worker
    }
    /// Returns the reconciliation-step ceiling.
    #[must_use]
    pub const fn reconciliation(self) -> u16 {
        self.reconciliation
    }
}

impl Default for RetryLimits {
    fn default() -> Self {
        Self { provider: 3, tool: 2, worker: 2, reconciliation: 8 }
    }
}

/// Deterministic resource ceilings reported by every scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    events: u32,
    evidence_bytes: u32,
    owned_processes: u16,
    cleanup_steps: u16,
    logical_ticks: u64,
}

impl ResourceLimits {
    /// Creates nonzero resource ceilings.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] when any limit is zero or exceeds its documented hard
    /// ceiling.
    pub fn new(
        events: u32,
        evidence_bytes: u32,
        owned_processes: u16,
        cleanup_steps: u16,
        logical_ticks: u64,
    ) -> Result<Self, ConfigurationError> {
        validate_u32("events", events, 1, 1_000_000)?;
        validate_u32("evidence_bytes", evidence_bytes, 1, 64 * 1024 * 1024)?;
        validate("owned_processes", owned_processes, 1, 4_096)?;
        validate("cleanup_steps", cleanup_steps, 1, 4_096)?;
        validate_u64("logical_ticks", logical_ticks, 1, 1_000_000_000)?;
        Ok(Self { events, evidence_bytes, owned_processes, cleanup_steps, logical_ticks })
    }

    /// Returns the event ceiling.
    #[must_use]
    pub const fn events(self) -> u32 {
        self.events
    }
    /// Returns the retained-evidence byte ceiling.
    #[must_use]
    pub const fn evidence_bytes(self) -> u32 {
        self.evidence_bytes
    }
    /// Returns the peak owned-process ceiling.
    #[must_use]
    pub const fn owned_processes(self) -> u16 {
        self.owned_processes
    }
    /// Returns the cleanup-step ceiling.
    #[must_use]
    pub const fn cleanup_steps(self) -> u16 {
        self.cleanup_steps
    }
    /// Returns the runtime-neutral logical-time ceiling.
    #[must_use]
    pub const fn logical_ticks(self) -> u64 {
        self.logical_ticks
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            events: 16_384,
            evidence_bytes: 4 * 1024 * 1024,
            owned_processes: 64,
            cleanup_steps: 64,
            logical_ticks: 100_000,
        }
    }
}

/// Complete immutable bounds for one qualification invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualificationConfig {
    max_scenarios: u16,
    max_milestones_per_scenario: u16,
    retries: RetryLimits,
    resources: ResourceLimits,
}

impl QualificationConfig {
    /// Creates validated qualification bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] when the scenario bound is outside
    /// `1..=HARD_MAX_SCENARIOS` or the milestone bound is outside
    /// `6..=HARD_MAX_MILESTONES`.
    pub fn new(
        max_scenarios: u16,
        max_milestones_per_scenario: u16,
        retries: RetryLimits,
        resources: ResourceLimits,
    ) -> Result<Self, ConfigurationError> {
        validate("max_scenarios", max_scenarios, 1, HARD_MAX_SCENARIOS)?;
        validate(
            "max_milestones_per_scenario",
            max_milestones_per_scenario,
            6,
            HARD_MAX_MILESTONES,
        )?;
        Ok(Self { max_scenarios, max_milestones_per_scenario, retries, resources })
    }

    /// Returns the scenario ceiling.
    #[must_use]
    pub const fn max_scenarios(self) -> u16 {
        self.max_scenarios
    }
    /// Returns the per-scenario milestone ceiling.
    #[must_use]
    pub const fn max_milestones_per_scenario(self) -> u16 {
        self.max_milestones_per_scenario
    }
    /// Returns retry ceilings.
    #[must_use]
    pub const fn retries(self) -> RetryLimits {
        self.retries
    }
    /// Returns resource ceilings.
    #[must_use]
    pub const fn resources(self) -> ResourceLimits {
        self.resources
    }
}

impl Default for QualificationConfig {
    fn default() -> Self {
        Self {
            max_scenarios: 64,
            max_milestones_per_scenario: 16,
            retries: RetryLimits::default(),
            resources: ResourceLimits::default(),
        }
    }
}

const fn validate(
    field: &'static str,
    value: u16,
    minimum: u16,
    maximum: u16,
) -> Result<(), ConfigurationError> {
    if value < minimum || value > maximum {
        Err(ConfigurationError::new(field, value as u64, minimum as u64, maximum as u64))
    } else {
        Ok(())
    }
}

const fn validate_u32(
    field: &'static str,
    value: u32,
    minimum: u32,
    maximum: u32,
) -> Result<(), ConfigurationError> {
    if value < minimum || value > maximum {
        Err(ConfigurationError::new(field, value as u64, minimum as u64, maximum as u64))
    } else {
        Ok(())
    }
}

const fn validate_u64(
    field: &'static str,
    value: u64,
    minimum: u64,
    maximum: u64,
) -> Result<(), ConfigurationError> {
    if value < minimum || value > maximum {
        Err(ConfigurationError::new(field, value, minimum, maximum))
    } else {
        Ok(())
    }
}
