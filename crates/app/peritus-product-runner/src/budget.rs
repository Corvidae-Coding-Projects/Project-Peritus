//! Cumulative accounting and generous hard ceilings for one complete product run.

use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

use peritus_agent::{DeveloperAccountingEvent, DeveloperUsage};
use peritus_types::ProviderProfileId;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

mod folder;
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
    /// Provider attempts admitted, including failed, cancelled and compaction requests.
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
    response_usage: DeveloperUsage,
    resources: RunResourceProbe,
    unavailable_providers: BTreeSet<ProviderProfileId>,
}

impl RunAccounting {
    pub fn new(workspace_root: &Path, max_elapsed: Duration) -> Result<Self, ProductRunnerError> {
        validate_run_horizon(max_elapsed)?;
        Ok(Self {
            started: Instant::now(),
            max_elapsed,
            progress: ProductRunProgress::default(),
            response_usage: DeveloperUsage::default(),
            resources: RunResourceProbe::new(workspace_root)?,
            unavailable_providers: BTreeSet::new(),
        })
    }

    pub(crate) fn record_event(
        &mut self,
        event: DeveloperAccountingEvent,
    ) -> Result<(), ProductRunnerError> {
        match event {
            DeveloperAccountingEvent::ModelRequest { retry } => {
                self.response_usage = DeveloperUsage::default();
                self.progress.model_requests = add_u32(self.progress.model_requests, 1)?;
                self.progress.retries = add_u32(self.progress.retries, u32::from(retry))?;
            }
            DeveloperAccountingEvent::ToolCall => {
                self.progress.tool_calls = add_u32(self.progress.tool_calls, 1)?;
            }
            DeveloperAccountingEvent::Compaction => {
                self.progress.compactions = add_u32(self.progress.compactions, 1)?;
            }
            DeveloperAccountingEvent::Usage(counters) => {
                let mut usage = DeveloperUsage::default();
                usage.observe(counters).map_err(|_| exhausted("provider usage overflowed"))?;
                self.record_usage(usage)?;
            }
        }
        // Per-event admission is cheap: do not recursively probe the workspace for every token
        // or tool. The ordinary role/settlement boundary still samples host resources.
        self.progress.elapsed_millis = millis(self.started.elapsed());
        budget_violation(self.progress, self.started.elapsed(), self.max_elapsed)
            .map_or(Ok(()), |detail| Err(exhausted(detail)))
    }

    fn record_usage(&mut self, usage: DeveloperUsage) -> Result<(), ProductRunnerError> {
        let previous = self.response_usage;
        let mut progress = self.progress;
        progress.input_tokens =
            replace_u64(progress.input_tokens, previous.input_tokens(), usage.input_tokens())?;
        progress.cached_input_tokens = replace_u64(
            progress.cached_input_tokens,
            previous.cached_input_tokens(),
            usage.cached_input_tokens(),
        )?;
        progress.output_tokens =
            replace_u64(progress.output_tokens, previous.output_tokens(), usage.output_tokens())?;
        progress.total_tokens =
            replace_u64(progress.total_tokens, previous.total_tokens(), usage.total_tokens())?;
        progress.provider_cost_microunits = replace_u64(
            progress.provider_cost_microunits,
            previous.provider_cost_microunits(),
            usage.provider_cost_microunits(),
        )?;
        progress.usage_observations = progress
            .usage_observations
            .checked_sub(previous.observations())
            .and_then(|value| value.checked_add(usage.observations()))
            .ok_or_else(|| exhausted("run usage observation counter overflowed"))?;
        self.progress = progress;
        self.response_usage = usage;
        Ok(())
    }

    pub(crate) fn record_role_retry(&mut self) -> Result<(), ProductRunnerError> {
        self.progress.retries = add_u32(self.progress.retries, 1)?;
        self.check()
    }

    pub fn record_provider_failover(&mut self) -> Result<(), ProductRunnerError> {
        self.progress.provider_failovers = add_u32(self.progress.provider_failovers, 1)?;
        self.check()
    }

    pub fn open_provider_circuit(&mut self, profile: ProviderProfileId) {
        self.unavailable_providers.insert(profile);
    }

    pub fn provider_circuit_open(&self, profile: ProviderProfileId) -> bool {
        self.unavailable_providers.contains(&profile)
    }

    pub fn close_provider_circuit(&mut self, profile: ProviderProfileId) {
        self.unavailable_providers.remove(&profile);
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

    /// Latest successfully recorded counters without performing another fallible host probe.
    #[must_use]
    pub const fn latest_snapshot(&self) -> ProductRunProgress {
        self.progress
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

fn replace_u64(total: u64, previous: u64, current: u64) -> Result<u64, ProductRunnerError> {
    total
        .checked_sub(previous)
        .and_then(|value| value.checked_add(current))
        .ok_or_else(|| exhausted("run accounting counter overflowed"))
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn exhausted(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "account complete coding run", detail)
}

fn invalid_horizon(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "configure coding run horizon",
        detail,
    )
}

#[cfg(test)]
mod tests;
