//! Conversion between persisted and live product-run progress.

use super::{PersistedProgress, RunProgress};

impl PersistedProgress {
    pub(super) fn from_run(value: &RunProgress) -> Self {
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
            last_event: value.last_event.clone(),
            provider_started_unix_millis: value.provider_started_unix_millis,
            _legacy_provider_deadline_seconds: None,
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
            last_event: if self.last_event.is_empty() {
                "restored run state".to_owned()
            } else {
                self.last_event
            },
            provider_started_unix_millis: self.provider_started_unix_millis,
            attempt_base: super::super::progress::AttemptBase::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_provider_elapsed_time_survives_persistence_and_reconnection() {
        let progress = RunProgress {
            started_unix_millis: 10,
            last_effect_unix_millis: 20,
            provider_started_unix_millis: Some(30),
            last_event: "provider turn started".to_owned(),
            model_requests: 4,
            tool_calls: 2,
            ..RunProgress::default()
        };
        let restored = PersistedProgress::from_run(&progress).into_run();
        assert_eq!(restored.started_unix_millis, 10);
        assert_eq!(restored.last_effect_unix_millis, 20);
        assert_eq!(restored.provider_started_unix_millis, Some(30));
        assert_eq!(restored.last_event, "provider turn started");
        assert_eq!(restored.model_requests, 4);
        assert_eq!(restored.tool_calls, 2);
    }

    #[test]
    fn legacy_provider_deadline_is_read_but_not_republished() {
        let persisted = PersistedProgress::from_run(&RunProgress::default());
        let mut value = serde_json::to_value(persisted).expect("serialize progress");
        value
            .as_object_mut()
            .expect("progress object")
            .insert("provider_deadline_seconds".to_owned(), serde_json::Value::from(600));
        let restored: PersistedProgress =
            serde_json::from_value(value).expect("read legacy provider deadline");
        let encoded = serde_json::to_value(PersistedProgress::from_run(&restored.into_run()))
            .expect("serialize migrated progress");
        assert!(encoded.get("provider_deadline_seconds").is_none());
    }
}
