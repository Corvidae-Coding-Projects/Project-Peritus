//! Read-only progress projection shared by the verification-facing API.

use super::ProductRunProgress;

impl ProductRunProgress {
    /// Provider requests completed or terminally observed.
    pub const fn model_requests(self) -> u32 {
        self.model_requests
    }
    /// Application tool calls completed.
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Checked provider retries completed.
    pub const fn retries(self) -> u32 {
        self.retries
    }
    /// Explicit switches to another configured provider.
    pub const fn provider_failovers(self) -> u32 {
        self.provider_failovers
    }
    /// Deterministic context compactions applied.
    pub const fn compactions(self) -> u32 {
        self.compactions
    }
    /// Provider-reported input tokens.
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    /// Provider-reported cache-read input tokens.
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    /// Provider-reported output tokens.
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    /// Explicit or conservatively derived aggregate tokens.
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    /// Provider-estimated cost in integer microunits.
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }
    /// Responses that supplied normalized usage.
    pub const fn usage_observations(self) -> u32 {
        self.usage_observations
    }
    /// Elapsed milliseconds at the latest effect boundary.
    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }
    /// Current regular-file bytes beneath the workspace, excluding Git object storage.
    pub const fn workspace_bytes(self) -> u64 {
        self.workspace_bytes
    }
    /// Positive workspace growth since this product-run attempt began.
    pub const fn workspace_growth_bytes(self) -> u64 {
        self.workspace_growth_bytes
    }
    /// Highest resident-memory observation at a completed effect boundary.
    pub const fn peak_rss_bytes(self) -> u64 {
        self.peak_rss_bytes
    }
}
