//! Durable product-run accounting and live user-facing liveness text.

use std::time::{SystemTime, UNIX_EPOCH};

use peritus_product_runner::{PRODUCT_RUN_MAX_ELAPSED, ProductRunProgress};

#[derive(Clone)]
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
    pub(super) workspace_bytes: u64,
    pub(super) workspace_growth_bytes: u64,
    pub(super) peak_rss_bytes: u64,
    pub(super) attempt_base: AttemptBase,
}

#[derive(Clone, Copy, Default)]
pub(super) struct AttemptBase {
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
    workspace_growth_bytes: u64,
    peak_rss_bytes: u64,
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
            workspace_bytes: 0,
            workspace_growth_bytes: 0,
            peak_rss_bytes: 0,
            attempt_base: AttemptBase::default(),
        }
    }
}

impl RunProgress {
    /// Starts a new execution attempt while retaining the complete run's accounting.
    pub(super) fn begin_attempt(&mut self) {
        self.attempt_base = AttemptBase {
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
            workspace_growth_bytes: self.workspace_growth_bytes,
            peak_rss_bytes: self.peak_rss_bytes,
        };
        self.last_effect_unix_millis = now_millis();
    }

    pub(super) fn observe(&mut self, progress: ProductRunProgress) {
        self.last_effect_unix_millis = now_millis();
        self.model_requests =
            self.attempt_base.model_requests.saturating_add(progress.model_requests());
        self.tool_calls = self.attempt_base.tool_calls.saturating_add(progress.tool_calls());
        self.retries = self.attempt_base.retries.saturating_add(progress.retries());
        self.provider_failovers =
            self.attempt_base.provider_failovers.saturating_add(progress.provider_failovers());
        self.compactions = self.attempt_base.compactions.saturating_add(progress.compactions());
        self.input_tokens = self.attempt_base.input_tokens.saturating_add(progress.input_tokens());
        self.cached_input_tokens =
            self.attempt_base.cached_input_tokens.saturating_add(progress.cached_input_tokens());
        self.output_tokens =
            self.attempt_base.output_tokens.saturating_add(progress.output_tokens());
        self.total_tokens = self.attempt_base.total_tokens.saturating_add(progress.total_tokens());
        self.provider_cost_microunits = self
            .attempt_base
            .provider_cost_microunits
            .saturating_add(progress.provider_cost_microunits());
        self.usage_observations =
            self.attempt_base.usage_observations.saturating_add(progress.usage_observations());
        self.workspace_bytes = progress.workspace_bytes();
        self.workspace_growth_bytes = self
            .attempt_base
            .workspace_growth_bytes
            .saturating_add(progress.workspace_growth_bytes());
        self.peak_rss_bytes = self.attempt_base.peak_rss_bytes.max(progress.peak_rss_bytes());
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
            format!("{} workspace growth", bytes(self.workspace_growth_bytes)),
            format!("{} observed memory", bytes(self.peak_rss_bytes)),
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

fn bytes(value: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if value >= GIB {
        format!("{}.{:01} GiB", value / GIB, (value % GIB) * 10 / GIB)
    } else if value >= MIB {
        format!("{}.{:01} MiB", value / MIB, (value % MIB) * 10 / MIB)
    } else if value >= KIB {
        format!("{}.{:01} KiB", value / KIB, (value % KIB) * 10 / KIB)
    } else {
        format!("{value} B")
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
        assert!(status.contains("workspace growth"));
        assert!(status.contains("observed memory"));
    }
}
