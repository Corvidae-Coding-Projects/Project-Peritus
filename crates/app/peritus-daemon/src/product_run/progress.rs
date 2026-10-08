//! Durable product-run accounting and live user-facing liveness text.

use std::time::{SystemTime, UNIX_EPOCH};

use peritus_product_runner::{
    ProductRunProgress, ResourceMeasurement, ResourceMeasurementStatus, ResourceObservationCause,
};

#[derive(Clone)]
pub(super) struct RunProgress {
    /// Immutable durable catalog membership assigned at first admission.
    pub(super) catalog_sequence: u64,
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
    pub(super) workspace_measurement: ResourceMeasurement,
    pub(super) workspace_growth_measurement: ResourceMeasurement,
    pub(super) peak_rss_measurement: ResourceMeasurement,
    pub(super) last_event: String,
    pub(super) provider_started_unix_millis: Option<u64>,
    pub(super) provider_request: Option<ProviderRequestIdentity>,
    pub(super) attempt_base: AttemptBase,
}

/// Restart-stable presentation identity for the one provider request currently owned by a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ProviderRequestIdentity {
    pub(super) role: u8,
    pub(super) turn: u16,
    pub(super) attempt: u64,
    pub(super) request_id_digest: [u8; 32],
    pub(super) request_fingerprint: [u8; 32],
    pub(super) provider_profile_id: [u8; 16],
    pub(super) provider_profile_revision: u64,
    pub(super) provider_name_digest: [u8; 32],
    pub(super) native_session_digest: Option<[u8; 32]>,
    pub(super) provider_selection_digest: Option<[u8; 32]>,
}

#[cfg(not(verus_only))]
impl From<peritus_agent::DeveloperProviderRequestIdentity> for ProviderRequestIdentity {
    fn from(value: peritus_agent::DeveloperProviderRequestIdentity) -> Self {
        Self {
            role: match value.role() {
                peritus_agent::DeveloperModelRole::Writer => 1,
                peritus_agent::DeveloperModelRole::Reviewer => 2,
                peritus_agent::DeveloperModelRole::Fixer => 3,
            },
            turn: value.turn(),
            attempt: value.attempt(),
            request_id_digest: value.request_id_digest().into_bytes(),
            request_fingerprint: value.request_fingerprint().into_bytes(),
            provider_profile_id: value.provider_profile_id().into_bytes(),
            provider_profile_revision: value.provider_profile_revision(),
            provider_name_digest: value.provider_name_digest().into_bytes(),
            native_session_digest: value.native_session_digest().map(|value| value.into_bytes()),
            provider_selection_digest: value
                .provider_selection_digest()
                .map(|value| value.into_bytes()),
        }
    }
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
            catalog_sequence: 0,
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
            workspace_measurement: ResourceMeasurement::unavailable(
                ResourceObservationCause::NotObserved,
            ),
            workspace_growth_measurement: ResourceMeasurement::unavailable(
                ResourceObservationCause::NotObserved,
            ),
            peak_rss_measurement: ResourceMeasurement::unavailable(
                ResourceObservationCause::NotObserved,
            ),
            last_event: "run started".to_owned(),
            provider_started_unix_millis: None,
            provider_request: None,
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
        "execution attempt started".clone_into(&mut self.last_event);
        self.provider_started_unix_millis = None;
        self.provider_request = None;
        self.workspace_measurement =
            ResourceMeasurement::unavailable(ResourceObservationCause::NotObserved);
        self.workspace_growth_measurement =
            ResourceMeasurement::unavailable(ResourceObservationCause::NotObserved);
        self.peak_rss_measurement =
            ResourceMeasurement::unavailable(ResourceObservationCause::NotObserved);
    }

    pub(super) fn observe(&mut self, progress: ProductRunProgress) {
        self.last_effect_unix_millis = now_millis();
        self.model_requests = self
            .model_requests
            .max(self.attempt_base.model_requests.saturating_add(progress.model_requests()));
        self.tool_calls =
            self.tool_calls.max(self.attempt_base.tool_calls.saturating_add(progress.tool_calls()));
        self.retries =
            self.retries.max(self.attempt_base.retries.saturating_add(progress.retries()));
        self.provider_failovers = self.provider_failovers.max(
            self.attempt_base.provider_failovers.saturating_add(progress.provider_failovers()),
        );
        self.compactions = self
            .compactions
            .max(self.attempt_base.compactions.saturating_add(progress.compactions()));
        self.input_tokens = self
            .input_tokens
            .max(self.attempt_base.input_tokens.saturating_add(progress.input_tokens()));
        self.cached_input_tokens = self.cached_input_tokens.max(
            self.attempt_base.cached_input_tokens.saturating_add(progress.cached_input_tokens()),
        );
        self.output_tokens = self
            .output_tokens
            .max(self.attempt_base.output_tokens.saturating_add(progress.output_tokens()));
        self.total_tokens = self
            .total_tokens
            .max(self.attempt_base.total_tokens.saturating_add(progress.total_tokens()));
        self.provider_cost_microunits = self.provider_cost_microunits.max(
            self.attempt_base
                .provider_cost_microunits
                .saturating_add(progress.provider_cost_microunits()),
        );
        self.usage_observations = self.usage_observations.max(
            self.attempt_base.usage_observations.saturating_add(progress.usage_observations()),
        );
        self.workspace_measurement = progress.workspace_measurement();
        self.workspace_growth_measurement = progress.workspace_growth_measurement();
        self.peak_rss_measurement = progress.peak_rss_measurement();
        if let Some(value) = self.workspace_measurement.measured_value() {
            self.workspace_bytes = value;
        }
        if let Some(value) = self.workspace_growth_measurement.measured_value() {
            self.workspace_growth_bytes = self
                .attempt_base
                .workspace_growth_bytes
                .saturating_add(value);
        }
        if let Some(value) = self.peak_rss_measurement.value() {
            self.peak_rss_bytes = self.attempt_base.peak_rss_bytes.max(value);
        }
        self.mark_event("runner progress checkpoint");
    }

    #[cfg(not(verus_only))]
    pub(super) fn begin_provider_request(
        &mut self,
        request: peritus_agent::DeveloperProviderRequestIdentity,
    ) {
        let now = now_millis();
        self.last_effect_unix_millis = now;
        self.provider_started_unix_millis = Some(now);
        self.provider_request = Some(request.into());
        self.model_requests = self.model_requests.saturating_add(1);
        "provider turn started".clone_into(&mut self.last_event);
    }

    pub(super) fn complete_provider_request(
        &mut self,
        usage: peritus_model_protocol::UsageCounters,
    ) {
        self.mark_event("provider turn completed");
        self.provider_started_unix_millis = None;
        self.provider_request = None;
        let values = [
            usage.input_tokens(),
            usage.cached_input_tokens(),
            usage.output_tokens(),
            usage.total_tokens(),
            usage.provider_cost_microunits(),
        ];
        if values.iter().any(Option::is_some) {
            self.usage_observations = self.usage_observations.saturating_add(1);
        }
        self.input_tokens = self.input_tokens.saturating_add(usage.input_tokens().unwrap_or(0));
        self.cached_input_tokens =
            self.cached_input_tokens.saturating_add(usage.cached_input_tokens().unwrap_or(0));
        self.output_tokens = self.output_tokens.saturating_add(usage.output_tokens().unwrap_or(0));
        self.total_tokens = self.total_tokens.saturating_add(usage.total_tokens().unwrap_or(0));
        self.provider_cost_microunits = self
            .provider_cost_microunits
            .saturating_add(usage.provider_cost_microunits().unwrap_or(0));
    }

    pub(super) fn begin_tool(&mut self, name: &str) {
        self.mark_event(&format!("tool started: {name}"));
        self.tool_calls = self.tool_calls.saturating_add(1);
    }

    pub(super) fn mark_event(&mut self, event: &str) {
        self.last_effect_unix_millis = now_millis();
        event.clone_into(&mut self.last_event);
    }

    pub(super) fn live_status(&self, base: &str) -> String {
        let now = now_millis();
        let elapsed = now.saturating_sub(self.started_unix_millis);
        let quiet = now.saturating_sub(self.last_effect_unix_millis);
        let mut fields = vec![base.to_owned()];
        if let Some(started) = self.provider_started_unix_millis {
            let provider_elapsed = now.saturating_sub(started);
            fields.push(format!("provider turn {}", duration(provider_elapsed)));
        }
        fields.push(format!("latest event {} · {} ago", self.last_event, duration(quiet)));
        let mut counters =
            vec![format!("{} requests", self.model_requests), format!("{} tools", self.tool_calls)];
        if self.retries > 0 {
            counters.push(format!("{} retries", self.retries));
        }
        if self.provider_failovers > 0 {
            counters.push(format!("{} provider switches", self.provider_failovers));
        }
        if self.compactions > 0 {
            counters.push(format!("{} compactions", self.compactions));
        }
        if self.usage_observations > 0 {
            counters.push(format!("{} tokens", count(self.total_tokens)));
            if self.cached_input_tokens > 0 {
                counters.push(format!("{} cached input", count(self.cached_input_tokens)));
            }
            if self.provider_cost_microunits > 0 {
                counters.push(format!("{} cost microunits", self.provider_cost_microunits));
            }
        }
        fields.push(format!("counters {}", counters.join(" · ")));
        fields.push(format!("elapsed {}", duration(elapsed)));
        fields.push(format!("workspace total {}", measurement(self.workspace_measurement)));
        fields.push(format!(
            "workspace growth {}",
            measurement(self.workspace_growth_measurement)
        ));
        fields.push(format!("observed memory {}", measurement(self.peak_rss_measurement)));
        if self.usage_observations == 0 {
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

fn measurement(value: ResourceMeasurement) -> String {
    let numeric = value.value().map(bytes);
    match value.status() {
        ResourceMeasurementStatus::Measured => {
            numeric.unwrap_or_else(|| "unavailable (invalid measured value)".to_owned())
        }
        ResourceMeasurementStatus::Partial => {
            let coverage = value.coverage();
            format!(
                "{} (partial: {} entries, {} directories, {} unavailable, {} pending; {})",
                numeric.map_or_else(
                    || "unavailable".to_owned(),
                    |value| format!("at least {value}"),
                ),
                coverage.entries_observed(),
                coverage.directories_completed(),
                coverage.items_unavailable(),
                coverage.directories_pending(),
                cause(value.cause()),
            )
        }
        ResourceMeasurementStatus::Unavailable => {
            format!("unavailable ({})", cause(value.cause()))
        }
    }
}

fn cause(value: Option<ResourceObservationCause>) -> String {
    value.map_or_else(
        || "cause was not retained".to_owned(),
        |value| match value {
            ResourceObservationCause::NotObserved => "not observed yet".to_owned(),
            ResourceObservationCause::LegacyUnspecified => {
                "legacy record did not retain measurement availability".to_owned()
            }
            ResourceObservationCause::BaselineEstablishedAfterStart => {
                "baseline completed after execution admission".to_owned()
            }
            ResourceObservationCause::NotAuthorized => {
                "recursive workspace observation was not authorized".to_owned()
            }
            ResourceObservationCause::ScanInProgress => "scan in progress".to_owned(),
            ResourceObservationCause::Cancelled => "observation cancelled".to_owned(),
            ResourceObservationCause::ArithmeticOverflow => {
                "numeric representation overflow".to_owned()
            }
            ResourceObservationCause::WorkerUnavailable => {
                "background observer unavailable".to_owned()
            }
            ResourceObservationCause::InvalidPlatformData => {
                "platform returned invalid data".to_owned()
            }
            ResourceObservationCause::PlatformCommandFailed { exit_code, signal } => {
                match (exit_code, signal) {
                    (Some(code), _) => format!("platform observer exited with {code}"),
                    (None, Some(signal)) => {
                        format!("platform observer terminated from signal {signal}")
                    }
                    (None, None) => {
                        "platform observer terminated without an exit code or signal".to_owned()
                    }
                }
            }
            ResourceObservationCause::Io { operation, kind, raw_os_error } => raw_os_error
                .map_or_else(
                    || format!("{operation:?} failed with {kind:?}"),
                    |code| format!("{operation:?} failed with {kind:?} (OS error {code})"),
                ),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_request() -> peritus_agent::DeveloperProviderRequestIdentity {
        peritus_agent::DeveloperProviderRequestIdentity::from_parts(
            peritus_agent::DeveloperModelRole::Writer,
            2,
            3,
            peritus_types::Sha256Digest::new([1; 32]),
            peritus_types::Sha256Digest::new([2; 32]),
            peritus_types::ProviderProfileId::new([3; 16]).expect("profile"),
            4,
            peritus_types::Sha256Digest::new([5; 32]),
            Some(peritus_types::Sha256Digest::new([6; 32])),
            Some(peritus_types::Sha256Digest::new([7; 32])),
        )
    }

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
        assert!(status.contains("latest event run started · 20s ago"));
        assert!(status.contains("counters 4 requests · 7 tools · 1 retries · 2 provider switches"));
        assert!(status.contains("workspace growth"));
        assert!(status.contains("observed memory"));
    }

    #[test]
    fn provider_events_are_live_and_completed_attempt_snapshots_cannot_roll_them_back() {
        let mut progress = RunProgress::default();
        progress.begin_provider_request(provider_request());
        progress.begin_tool("workspace_read");
        let status = progress.live_status("Writer is working");
        assert!(status.contains("counters 1 requests · 1 tools"));
        assert!(status.contains("latest event tool started: workspace_read ·"));
        assert!(status.contains("provider turn"));
        assert!(!status.contains("deadline"));
        assert!(!status.contains("run horizon"));

        progress.observe(ProductRunProgress::default());
        assert_eq!(progress.model_requests, 1);
        assert_eq!(progress.tool_calls, 1);
        progress.complete_provider_request(peritus_model_protocol::UsageCounters::new(
            Some(8),
            Some(3),
            None,
            Some(5),
            None,
            None,
            Some(13),
            Some(2),
        ));
        assert_eq!(progress.total_tokens, 13);
        assert_eq!(progress.usage_observations, 1);
        assert!(progress.provider_started_unix_millis.is_none());
        assert!(progress.provider_request.is_none());
    }
}
