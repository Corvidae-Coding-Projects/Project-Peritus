//! Cumulative accounting and generous hard ceilings for one complete product run.

use std::time::{Duration, Instant};

use peritus_agent::DeveloperLoopOutcome;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

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
}

pub struct RunAccounting {
    started: Instant,
    progress: ProductRunProgress,
}

impl RunAccounting {
    pub fn new() -> Self {
        Self { started: Instant::now(), progress: ProductRunProgress::default() }
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
        let violation = if self.started.elapsed() > PRODUCT_RUN_MAX_ELAPSED {
            Some("the eight-hour uninterrupted run horizon was exhausted")
        } else if self.progress.model_requests > PRODUCT_RUN_MAX_MODEL_REQUESTS {
            Some("the cumulative provider-request budget was exhausted")
        } else if self.progress.tool_calls > PRODUCT_RUN_MAX_TOOL_CALLS {
            Some("the cumulative application-tool budget was exhausted")
        } else if self.progress.total_tokens > PRODUCT_RUN_MAX_TOTAL_TOKENS {
            Some("the cumulative model-token budget was exhausted")
        } else if self.progress.provider_cost_microunits > PRODUCT_RUN_MAX_COST_MICROUNITS {
            Some("the cumulative provider-estimated cost budget was exhausted")
        } else {
            None
        };
        violation.map_or(Ok(()), |detail| Err(exhausted(detail)))
    }

    pub fn snapshot(&mut self) -> Result<ProductRunProgress, ProductRunnerError> {
        self.check()?;
        Ok(self.progress)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_outcomes_accumulate_requests_tools_retries_and_compactions() {
        let mut accounting = RunAccounting::new();
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
        let mut accounting = RunAccounting::new();
        accounting.record_provider_failover().expect("record failover");
        let progress = accounting.snapshot().expect("bounded progress");
        assert_eq!(progress.provider_failovers(), 1);
        assert_eq!(progress.retries(), 0);
    }
}
