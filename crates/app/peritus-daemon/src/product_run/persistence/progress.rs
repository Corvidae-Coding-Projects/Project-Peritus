//! Conversion between persisted and live product-run progress.

use super::{PersistedProgress, RunProgress};

impl PersistedProgress {
    pub(super) const fn from_run(value: &RunProgress) -> Self {
        Self {
            started_unix_millis: value.started_unix_millis,
            last_effect_unix_millis: value.last_effect_unix_millis,
            model_requests: value.model_requests,
            tool_calls: value.tool_calls,
            retries: value.retries,
            provider_failovers: value.provider_failovers,
            compactions: value.compactions,
            input_tokens: value.input_tokens,
            cached_input_tokens: value.cached_input_tokens,
            output_tokens: value.output_tokens,
            total_tokens: value.total_tokens,
            provider_cost_microunits: value.provider_cost_microunits,
            usage_observations: value.usage_observations,
            workspace_bytes: value.workspace_bytes,
            workspace_growth_bytes: value.workspace_growth_bytes,
            peak_rss_bytes: value.peak_rss_bytes,
        }
    }

    pub(super) fn into_run(self) -> RunProgress {
        if self.started_unix_millis == 0 || self.last_effect_unix_millis == 0 {
            return RunProgress::default();
        }
        RunProgress {
            started_unix_millis: self.started_unix_millis,
            last_effect_unix_millis: self.last_effect_unix_millis,
            model_requests: self.model_requests,
            tool_calls: self.tool_calls,
            retries: self.retries,
            provider_failovers: self.provider_failovers,
            compactions: self.compactions,
            input_tokens: self.input_tokens,
            cached_input_tokens: self.cached_input_tokens,
            output_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
            provider_cost_microunits: self.provider_cost_microunits,
            usage_observations: self.usage_observations,
            workspace_bytes: self.workspace_bytes,
            workspace_growth_bytes: self.workspace_growth_bytes,
            peak_rss_bytes: self.peak_rss_bytes,
        }
    }
}
