//! Durable product-run accounting and live user-facing liveness text.

use std::time::{SystemTime, UNIX_EPOCH};

use peritus_product_runner::{PRODUCT_RUN_MAX_ELAPSED, ProductRunProgress};

pub(super) struct RunProgress {
    pub(super) started_unix_millis: u64,
    pub(super) last_effect_unix_millis: u64,
    pub(super) model_requests: u32,
    pub(super) tool_calls: u32,
    pub(super) retries: u32,
    pub(super) provider_failovers: u32,
    pub(super) compactions: u32,
    pub(super) input_tokens: u64,
    pub(super) cached_input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) provider_cost_microunits: u64,
    pub(super) usage_observations: u32,
}

impl Default for RunProgress {
    fn default() -> Self {
        let now = now_millis();
        Self {
            started_unix_millis: now,
            last_effect_unix_millis: now,
            model_requests: 0,
            tool_calls: 0,
            retries: 0,
            provider_failovers: 0,
            compactions: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            provider_cost_microunits: 0,
            usage_observations: 0,
        }
    }
}

impl RunProgress {
    pub(super) fn observe(&mut self, progress: ProductRunProgress) {
        self.last_effect_unix_millis = now_millis();
        self.model_requests = progress.model_requests();
        self.tool_calls = progress.tool_calls();
        self.retries = progress.retries();
        self.provider_failovers = progress.provider_failovers();
        self.compactions = progress.compactions();
        self.input_tokens = progress.input_tokens();
        self.cached_input_tokens = progress.cached_input_tokens();
        self.output_tokens = progress.output_tokens();
        self.total_tokens = progress.total_tokens();
        self.provider_cost_microunits = progress.provider_cost_microunits();
        self.usage_observations = progress.usage_observations();
    }

    pub(super) fn live_status(&self, base: &str) -> String {
        let now = now_millis();
        let elapsed = now.saturating_sub(self.started_unix_millis);
        let quiet = now.saturating_sub(self.last_effect_unix_millis);
        let horizon = u64::try_from(PRODUCT_RUN_MAX_ELAPSED.as_millis()).unwrap_or(u64::MAX);
        let remaining = horizon.saturating_sub(elapsed);
        let mut fields = vec![
            base.to_owned(),
            format!("elapsed {}", duration(elapsed)),
            format!("last durable progress {} ago", duration(quiet)),
            format!("run horizon {} remaining", duration(remaining)),
            format!("{} provider requests", self.model_requests),
            format!("{} tool calls", self.tool_calls),
        ];
        if self.retries > 0 {
            fields.push(format!("{} retries", self.retries));
        }
        if self.provider_failovers > 0 {
            fields.push(format!("{} provider switches", self.provider_failovers));
        }
        if self.compactions > 0 {
            fields.push(format!("{} context compactions", self.compactions));
        }
        if self.usage_observations > 0 {
            fields.push(format!("{} tokens", count(self.total_tokens)));
            if self.cached_input_tokens > 0 {
                fields.push(format!("{} cached input", count(self.cached_input_tokens)));
            }
            if self.provider_cost_microunits > 0 {
                fields.push(format!("{} cost microunits", self.provider_cost_microunits));
            }
        } else {
            fields.push("provider did not report token usage yet".to_owned());
        }
        fields.join(" | ")
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn duration(millis: u64) -> String {
    let seconds = millis / 1_000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

fn count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{}.{:01}M", value / 1_000_000, (value % 1_000_000) / 100_000)
    } else if value >= 1_000 {
        format!("{}.{:01}k", value / 1_000, (value % 1_000) / 100)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_status_distinguishes_silence_from_failure() {
        let progress = RunProgress {
            started_unix_millis: now_millis().saturating_sub(65_000),
            last_effect_unix_millis: now_millis().saturating_sub(20_000),
            model_requests: 4,
            tool_calls: 7,
            retries: 1,
            provider_failovers: 2,
            ..RunProgress::default()
        };
        let status = progress.live_status("Writer is working");
        assert!(status.contains("elapsed 1m 5s"));
        assert!(status.contains("last durable progress 20s ago"));
        assert!(status.contains("4 provider requests"));
        assert!(status.contains("1 retries"));
        assert!(status.contains("2 provider switches"));
    }
}
