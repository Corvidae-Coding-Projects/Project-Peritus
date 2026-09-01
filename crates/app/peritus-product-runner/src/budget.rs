//! Cumulative accounting and generous hard ceilings for one complete product run.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use peritus_agent::DeveloperLoopOutcome;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

#[path = "resource_probe.rs"]
mod resource_probe;

use resource_probe::RunResourceProbe;

/// Maximum wall-clock duration of one uninterrupted product-run attempt.
pub const PRODUCT_RUN_MAX_ELAPSED: Duration = Duration::from_hours(8);
/// Maximum provider requests across designer, writer, reviewer, and fixer roles.
pub const PRODUCT_RUN_MAX_MODEL_REQUESTS: u32 = 4_096;
/// Maximum application tool calls across all roles.
pub const PRODUCT_RUN_MAX_TOOL_CALLS: u32 = 20_000;
/// Maximum provider-reported or conservatively derived tokens across all roles.
pub const PRODUCT_RUN_MAX_TOTAL_TOKENS: u64 = 100_000_000;
/// Maximum provider-estimated cost in integer microunits when the provider reports it.
pub const PRODUCT_RUN_MAX_COST_MICROUNITS: u64 = 500_000_000;
/// Maximum resident memory observed for the harness process at completed effect boundaries.
pub const PRODUCT_RUN_MAX_PEAK_RSS_BYTES: u64 = 12 * 1024 * 1024 * 1024;
/// Maximum regular-file growth beneath the managed workspace during one run.
pub const PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// Monotonic aggregate progress for one complete product-run attempt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProductRunProgress {
    model_requests: u32,
    tool_calls: u32,
    retries: u32,
    provider_failovers: u32,
    compactions: u32,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    provider_cost_microunits: u64,
    usage_observations: u32,
    elapsed_millis: u64,
    workspace_bytes: u64,
    workspace_growth_bytes: u64,
    peak_rss_bytes: u64,
}

impl ProductRunProgress {
    /// Provider requests completed or terminally observed.
    #[must_use]
    pub const fn model_requests(self) -> u32 {
        self.model_requests
    }

    /// Application tool calls completed.
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }

    /// Additional provider attempts caused by checked retry policy.
    #[must_use]
    pub const fn retries(self) -> u32 {
        self.retries
    }

    /// Explicit switches to another configured provider after ordinary recovery was exhausted.
    #[must_use]
    pub const fn provider_failovers(self) -> u32 {
        self.provider_failovers
    }

    /// Deterministic context compactions applied.
    #[must_use]
    pub const fn compactions(self) -> u32 {
        self.compactions
    }

    /// Provider-reported input tokens.
    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }

    /// Provider-reported cache-read input tokens.
    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }

    /// Provider-reported output tokens.
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }

    /// Explicit or conservatively derived aggregate tokens.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }

    /// Provider-estimated cost in integer microunits.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }

    /// Responses that supplied at least one normalized usage counter.
    #[must_use]
    pub const fn usage_observations(self) -> u32 {
        self.usage_observations
    }

    /// Wall-clock time observed at the latest completed effect boundary.
    #[must_use]
    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }

    /// Current regular-file bytes beneath the workspace, excluding Git object storage.
    #[must_use]
    pub const fn workspace_bytes(self) -> u64 {
        self.workspace_bytes
    }

    /// Positive workspace growth since this product-run attempt began.
    #[must_use]
    pub const fn workspace_growth_bytes(self) -> u64 {
        self.workspace_growth_bytes
    }

    /// Highest resident-memory observation for the harness process.
    #[must_use]
    pub const fn peak_rss_bytes(self) -> u64 {
        self.peak_rss_bytes
    }
}

pub struct RunAccounting {
    started: Instant,
    max_elapsed: Duration,
    progress: ProductRunProgress,
    resources: RunResourceProbe,
}

impl RunAccounting {
    pub fn new(workspace_root: &Path, max_elapsed: Duration) -> Result<Self, ProductRunnerError> {
        validate_run_horizon(max_elapsed)?;
        Ok(Self {
            started: Instant::now(),
            max_elapsed,
            progress: ProductRunProgress::default(),
            resources: RunResourceProbe::new(workspace_root)?,
        })
    }

    pub fn record(&mut self, outcome: &DeveloperLoopOutcome) -> Result<(), ProductRunnerError> {
        let retries = u32::from(outcome.retries);
        let requests = u32::from(outcome.model_turns)
            .checked_add(retries)
            .ok_or_else(|| exhausted("provider request counter overflowed"))?;
        self.progress.model_requests = add_u32(self.progress.model_requests, requests)?;
        self.progress.tool_calls = add_u32(self.progress.tool_calls, outcome.tool_calls)?;
        self.progress.retries = add_u32(self.progress.retries, retries)?;
        self.progress.compactions =
            add_u32(self.progress.compactions, u32::from(outcome.compactions))?;
        let usage = outcome.usage;
        self.progress.input_tokens = add_u64(self.progress.input_tokens, usage.input_tokens())?;
        self.progress.cached_input_tokens =
            add_u64(self.progress.cached_input_tokens, usage.cached_input_tokens())?;
        self.progress.output_tokens = add_u64(self.progress.output_tokens, usage.output_tokens())?;
        self.progress.total_tokens = add_u64(self.progress.total_tokens, usage.total_tokens())?;
        self.progress.provider_cost_microunits =
            add_u64(self.progress.provider_cost_microunits, usage.provider_cost_microunits())?;
        self.progress.usage_observations =
            add_u32(self.progress.usage_observations, usage.observations())?;
        self.check()
    }

    pub fn record_provider_failover(&mut self) -> Result<(), ProductRunnerError> {
        self.progress.provider_failovers = add_u32(self.progress.provider_failovers, 1)?;
        self.check()
    }

    pub fn check(&mut self) -> Result<(), ProductRunnerError> {
        self.progress.elapsed_millis = millis(self.started.elapsed());
        let resources = self.resources.observe()?;
        self.progress.workspace_bytes = resources.workspace;
        self.progress.workspace_growth_bytes = resources.growth;
        self.progress.peak_rss_bytes = self.progress.peak_rss_bytes.max(resources.peak_rss);
        let violation = budget_violation(self.progress, self.started.elapsed(), self.max_elapsed);
        violation.map_or(Ok(()), |detail| Err(exhausted(detail)))
    }

    pub fn snapshot(&mut self) -> Result<ProductRunProgress, ProductRunnerError> {
        self.check()?;
        Ok(self.progress)
    }

    pub fn remaining(&self) -> Duration {
        self.max_elapsed.saturating_sub(self.started.elapsed())
    }
}

pub fn validate_run_horizon(max_elapsed: Duration) -> Result<(), ProductRunnerError> {
    if max_elapsed.is_zero() {
        Err(invalid_horizon("configured run horizon must be greater than zero"))
    } else if max_elapsed > PRODUCT_RUN_MAX_ELAPSED {
        Err(invalid_horizon("configured run horizon exceeds the eight-hour hard ceiling"))
    } else {
        Ok(())
    }
}

fn budget_violation(
    progress: ProductRunProgress,
    elapsed: Duration,
    max_elapsed: Duration,
) -> Option<&'static str> {
    if elapsed > max_elapsed {
        Some("the configured run horizon was exhausted")
    } else if progress.model_requests > PRODUCT_RUN_MAX_MODEL_REQUESTS {
        Some("the cumulative provider-request budget was exhausted")
    } else if progress.tool_calls > PRODUCT_RUN_MAX_TOOL_CALLS {
        Some("the cumulative application-tool budget was exhausted")
    } else if progress.total_tokens > PRODUCT_RUN_MAX_TOTAL_TOKENS {
        Some("the cumulative model-token budget was exhausted")
    } else if progress.provider_cost_microunits > PRODUCT_RUN_MAX_COST_MICROUNITS {
        Some("the cumulative provider-estimated cost budget was exhausted")
    } else if progress.peak_rss_bytes > PRODUCT_RUN_MAX_PEAK_RSS_BYTES {
        Some("the product-run peak resident-memory budget was exhausted")
    } else if progress.workspace_growth_bytes > PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES {
        Some("the product-run workspace-growth budget was exhausted")
    } else {
        None
    }
}

fn add_u32(left: u32, right: u32) -> Result<u32, ProductRunnerError> {
    left.checked_add(right).ok_or_else(|| exhausted("run accounting counter overflowed"))
}

fn add_u64(left: u64, right: u64) -> Result<u64, ProductRunnerError> {
    left.checked_add(right).ok_or_else(|| exhausted("run accounting counter overflowed"))
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn exhausted(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "account complete coding run", detail)
}

fn invalid_horizon(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "configure coding run horizon", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_outcomes_accumulate_requests_tools_retries_and_compactions() {
        let temporary = tempfile::tempdir().expect("workspace");
        let mut accounting =
            RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
        accounting
            .record(&DeveloperLoopOutcome {
                text: "done".to_owned(),
                model_turns: 48,
                tool_calls: 512,
                compactions: 2,
                retries: 3,
                usage: peritus_agent::DeveloperUsage::default(),
                messages: Vec::new(),
            })
            .expect("record bounded role outcome");
        let progress = accounting.snapshot().expect("bounded progress");

        assert_eq!(progress.model_requests(), 51);
        assert_eq!(progress.tool_calls(), 512);
        assert_eq!(progress.retries(), 3);
        assert_eq!(progress.compactions(), 2);
    }

    #[test]
    fn provider_failovers_are_counted_separately_from_same_provider_retries() {
        let temporary = tempfile::tempdir().expect("workspace");
        let mut accounting =
            RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
        accounting.record_provider_failover().expect("record failover");
        let progress = accounting.snapshot().expect("bounded progress");
        assert_eq!(progress.provider_failovers(), 1);
        assert_eq!(progress.retries(), 0);
    }

    #[test]
    fn workspace_growth_and_peak_memory_are_observed_at_effect_boundaries() {
        let temporary = tempfile::tempdir().expect("workspace");
        let mut accounting =
            RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
        std::fs::write(temporary.path().join("candidate.bin"), vec![0_u8; 4096])
            .expect("candidate");

        let progress = accounting.snapshot().expect("resource snapshot");

        assert_eq!(progress.workspace_growth_bytes(), 4096);
        assert_eq!(progress.workspace_bytes(), 4096);
        assert!(progress.peak_rss_bytes() > 0);
    }

    #[test]
    fn memory_and_workspace_growth_have_distinct_hard_failures() {
        let memory = ProductRunProgress {
            peak_rss_bytes: PRODUCT_RUN_MAX_PEAK_RSS_BYTES + 1,
            ..ProductRunProgress::default()
        };
        assert_eq!(
            budget_violation(memory, Duration::ZERO, PRODUCT_RUN_MAX_ELAPSED),
            Some("the product-run peak resident-memory budget was exhausted")
        );

        let workspace = ProductRunProgress {
            workspace_growth_bytes: PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES + 1,
            ..ProductRunProgress::default()
        };
        assert_eq!(
            budget_violation(workspace, Duration::ZERO, PRODUCT_RUN_MAX_ELAPSED),
            Some("the product-run workspace-growth budget was exhausted")
        );
    }

    #[test]
    fn caller_run_horizon_is_bounded_and_drives_elapsed_budget() {
        assert!(validate_run_horizon(Duration::ZERO).is_err());
        assert!(validate_run_horizon(PRODUCT_RUN_MAX_ELAPSED + Duration::from_secs(1)).is_err());
        assert!(validate_run_horizon(Duration::from_mins(1)).is_ok());
        assert_eq!(
            budget_violation(
                ProductRunProgress::default(),
                Duration::from_secs(61),
                Duration::from_mins(1),
            ),
            Some("the configured run horizon was exhausted")
        );
    }
}
