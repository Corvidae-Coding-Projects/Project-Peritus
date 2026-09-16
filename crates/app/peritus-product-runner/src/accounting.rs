//! Production numeric accounting shared by ordinary execution and Verus.

mod limits;
mod usage;
mod work;

pub use limits::BudgetViolation;
pub use limits::{
    PRODUCT_RUN_MAX_COST_MICROUNITS, PRODUCT_RUN_MAX_MODEL_REQUESTS,
    PRODUCT_RUN_MAX_PEAK_RSS_BYTES, PRODUCT_RUN_MAX_TOOL_CALLS, PRODUCT_RUN_MAX_TOTAL_TOKENS,
    PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES,
};
pub use usage::UsageSnapshot;
use vstd::prelude::*;
pub use work::WorkEvent;

verus! {

/// Aggregate progress for one complete product-run attempt.
///
/// Usage replaces each response snapshot, including a later explicit total that corrects a
/// previously derived total. Resource samples describe completed effect boundaries.
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
    /// Specification view of model requests.
    pub closed spec fn spec_model_requests(self) -> u32 { self.model_requests }

    /// Provider attempts admitted, including failed and cancelled requests.
    #[must_use]
    pub const fn model_requests(self) -> (value: u32)
        ensures value == self.spec_model_requests(),
    { self.model_requests }

    /// Specification view of tool calls.
    pub closed spec fn spec_tool_calls(self) -> u32 { self.tool_calls }

    /// Application tool calls completed.
    #[must_use]
    pub const fn tool_calls(self) -> (value: u32)
        ensures value == self.spec_tool_calls(),
    { self.tool_calls }

    /// Specification view of retries.
    pub closed spec fn spec_retries(self) -> u32 { self.retries }

    /// Additional provider attempts caused by checked retry policy.
    #[must_use]
    pub const fn retries(self) -> (value: u32)
        ensures value == self.spec_retries(),
    { self.retries }

    /// Specification view of provider failovers.
    pub closed spec fn spec_provider_failovers(self) -> u32 { self.provider_failovers }

    /// Explicit switches to another configured provider.
    #[must_use]
    pub const fn provider_failovers(self) -> (value: u32)
        ensures value == self.spec_provider_failovers(),
    { self.provider_failovers }

    /// Specification view of compactions.
    pub closed spec fn spec_compactions(self) -> u32 { self.compactions }

    /// Deterministic context compactions applied.
    #[must_use]
    pub const fn compactions(self) -> (value: u32)
        ensures value == self.spec_compactions(),
    { self.compactions }

    /// Specification view of input tokens.
    pub closed spec fn spec_input_tokens(self) -> u64 { self.input_tokens }

    /// Provider-reported input tokens.
    #[must_use]
    pub const fn input_tokens(self) -> (value: u64)
        ensures value == self.spec_input_tokens(),
    { self.input_tokens }

    /// Specification view of cached input tokens.
    pub closed spec fn spec_cached_input_tokens(self) -> u64 { self.cached_input_tokens }

    /// Provider-reported cache-read input tokens.
    #[must_use]
    pub const fn cached_input_tokens(self) -> (value: u64)
        ensures value == self.spec_cached_input_tokens(),
    { self.cached_input_tokens }

    /// Specification view of output tokens.
    pub closed spec fn spec_output_tokens(self) -> u64 { self.output_tokens }

    /// Provider-reported output tokens.
    #[must_use]
    pub const fn output_tokens(self) -> (value: u64)
        ensures value == self.spec_output_tokens(),
    { self.output_tokens }

    /// Specification view of total tokens.
    pub closed spec fn spec_total_tokens(self) -> u64 { self.total_tokens }

    /// Explicit or conservatively derived aggregate tokens.
    #[must_use]
    pub const fn total_tokens(self) -> (value: u64)
        ensures value == self.spec_total_tokens(),
    { self.total_tokens }

    /// Specification view of provider cost microunits.
    pub closed spec fn spec_provider_cost_microunits(self) -> u64 { self.provider_cost_microunits }

    /// Provider-estimated cost in integer microunits.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> (value: u64)
        ensures value == self.spec_provider_cost_microunits(),
    { self.provider_cost_microunits }

    /// Specification view of usage observations.
    pub closed spec fn spec_usage_observations(self) -> u32 { self.usage_observations }

    /// Responses that supplied at least one normalized usage counter.
    #[must_use]
    pub const fn usage_observations(self) -> (value: u32)
        ensures value == self.spec_usage_observations(),
    { self.usage_observations }

    /// Specification view of elapsed millis.
    pub closed spec fn spec_elapsed_millis(self) -> u64 { self.elapsed_millis }

    /// Wall-clock time observed at the latest completed effect boundary.
    #[must_use]
    pub const fn elapsed_millis(self) -> (value: u64)
        ensures value == self.spec_elapsed_millis(),
    { self.elapsed_millis }

    /// Specification view of workspace bytes.
    pub closed spec fn spec_workspace_bytes(self) -> u64 { self.workspace_bytes }

    /// Current regular-file bytes beneath the workspace.
    #[must_use]
    pub const fn workspace_bytes(self) -> (value: u64)
        ensures value == self.spec_workspace_bytes(),
    { self.workspace_bytes }

    /// Specification view of workspace growth bytes.
    pub closed spec fn spec_workspace_growth_bytes(self) -> u64 { self.workspace_growth_bytes }

    /// Positive workspace growth since this run attempt began.
    #[must_use]
    pub const fn workspace_growth_bytes(self) -> (value: u64)
        ensures value == self.spec_workspace_growth_bytes(),
    { self.workspace_growth_bytes }

    /// Specification view of peak rss bytes.
    pub closed spec fn spec_peak_rss_bytes(self) -> u64 { self.peak_rss_bytes }

    /// Highest resident-memory observation for the harness process.
    #[must_use]
    pub const fn peak_rss_bytes(self) -> (value: u64)
        ensures value == self.spec_peak_rss_bytes(),
    { self.peak_rss_bytes }

}

/// Numeric state retained across response snapshots and request boundaries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccountingState {
    pub(crate) progress: ProductRunProgress,
    pub(crate) response_usage: UsageSnapshot,
}

/// Arithmetic rejection before any numeric state is published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountingError {
    /// A token, cost, or work counter cannot represent the requested update.
    CounterOverflow,
    /// The cumulative observation count cannot represent the requested replacement.
    ObservationOverflow,
}

} // verus!
