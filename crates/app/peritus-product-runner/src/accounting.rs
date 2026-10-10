//! Production numeric accounting shared by ordinary execution and Verus.

mod limits;
mod usage;
mod work;

pub use limits::BudgetViolation;
pub use usage::UsageSnapshot;

pub use work::WorkEvent;

#[doc = " Aggregate progress for one complete product-run attempt."]
#[doc = ""]
#[doc = " Usage replaces each response snapshot, including a later explicit total that corrects a"]
#[doc = " previously derived total. Resource samples describe completed effect boundaries."]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProductRunProgress {
    pub(crate) model_requests: u32,
    pub(crate) tool_calls: u32,
    pub(crate) retries: u32,
    pub(crate) provider_failovers: u32,
    pub(crate) compactions: u32,
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) total_tokens: u64,
    pub(crate) provider_cost_microunits: u64,
    pub(crate) usage_observations: u32,
    pub(crate) elapsed_millis: u64,
    pub(crate) workspace_bytes: u64,
    pub(crate) workspace_growth_bytes: u64,
    pub(crate) peak_rss_bytes: u64,
}
impl ProductRunProgress {
    #[doc = " Provider attempts admitted, including failed and cancelled requests."]
    #[must_use]
    pub const fn model_requests(self) -> u32 {
        self.model_requests
    }
    #[doc = " Application tool calls completed."]
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    #[doc = " Additional provider attempts caused by checked retry policy."]
    #[must_use]
    pub const fn retries(self) -> u32 {
        self.retries
    }
    #[doc = " Explicit switches to another configured provider."]
    #[must_use]
    pub const fn provider_failovers(self) -> u32 {
        self.provider_failovers
    }
    #[doc = " Deterministic context compactions applied."]
    #[must_use]
    pub const fn compactions(self) -> u32 {
        self.compactions
    }
    #[doc = " Provider-reported input tokens."]
    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    #[doc = " Provider-reported cache-read input tokens."]
    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    #[doc = " Provider-reported output tokens."]
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    #[doc = " Explicit or conservatively derived aggregate tokens."]
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    #[doc = " Provider-estimated cost in integer microunits."]
    #[must_use]
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }
    #[doc = " Responses that supplied at least one normalized usage counter."]
    #[must_use]
    pub const fn usage_observations(self) -> u32 {
        self.usage_observations
    }
    #[doc = " Wall-clock time observed at the latest completed effect boundary."]
    #[must_use]
    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }
    #[doc = " Current regular-file bytes beneath the workspace."]
    #[must_use]
    pub const fn workspace_bytes(self) -> u64 {
        self.workspace_bytes
    }
    #[doc = " Positive workspace growth since this run attempt began."]
    #[must_use]
    pub const fn workspace_growth_bytes(self) -> u64 {
        self.workspace_growth_bytes
    }
    #[doc = " Highest resident-memory observation for the harness process."]
    #[must_use]
    pub const fn peak_rss_bytes(self) -> u64 {
        self.peak_rss_bytes
    }
}
#[doc = " Numeric state retained across response snapshots and request boundaries."]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccountingState {
    pub(crate) progress: ProductRunProgress,
    pub(crate) response_usage: UsageSnapshot,
}
#[doc = " Arithmetic rejection before any numeric state is published."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountingError {
    #[doc = " A token, cost, or work counter cannot represent the requested update."]
    CounterOverflow,
    #[doc = " The cumulative observation count cannot represent the requested replacement."]
    ObservationOverflow,
}
