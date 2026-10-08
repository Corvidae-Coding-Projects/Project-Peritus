//! Conversion between persisted and live product-run progress.

use super::{
    PersistedProgress, PersistedProviderRequest, PersistedResourceCause, PersistedResourceCoverage,
    PersistedResourceIoKind, PersistedResourceMeasurement, PersistedResourceOperation,
    PersistedResourceTelemetry, RunProgress,
};
use peritus_product_runner::{
    ResourceIoErrorKind, ResourceMeasurement, ResourceMeasurementStatus,
    ResourceObservationCause, ResourceObservationCoverage, ResourceObservationOperation,
};
use super::super::progress::ProviderRequestIdentity;

impl PersistedProgress {
    pub(super) fn from_run(value: &RunProgress) -> Self {
        Self {
            catalog_sequence: value.catalog_sequence,
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
            resource_telemetry: Some(PersistedResourceTelemetry {
                workspace: PersistedResourceMeasurement::from_domain(
                    value.workspace_measurement,
                ),
                workspace_growth: PersistedResourceMeasurement::from_domain(
                    value.workspace_growth_measurement,
                ),
                peak_rss: PersistedResourceMeasurement::from_domain(value.peak_rss_measurement),
            }),
            last_event: value.last_event.clone(),
            provider_started_unix_millis: value.provider_started_unix_millis,
            provider_request: value.provider_request.map(PersistedProviderRequest::from_domain),
            _legacy_provider_deadline_seconds: None,
        }
    }

    pub(super) fn into_run(self) -> RunProgress {
        let (workspace_measurement, workspace_growth_measurement, peak_rss_measurement) = self
            .resource_telemetry
            .map(PersistedResourceTelemetry::into_domain)
            .unwrap_or_else(legacy_resource_telemetry);
        if self.started_unix_millis == 0 || self.last_effect_unix_millis == 0 {
            let mut progress = RunProgress::default();
            progress.catalog_sequence = self.catalog_sequence;
            progress.workspace_measurement = workspace_measurement;
            progress.workspace_growth_measurement = workspace_growth_measurement;
            progress.peak_rss_measurement = peak_rss_measurement;
            return progress;
        }
        RunProgress {
            catalog_sequence: self.catalog_sequence,
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
            workspace_measurement,
            workspace_growth_measurement,
            peak_rss_measurement,
            last_event: if self.last_event.is_empty() {
                "restored run state".to_owned()
            } else {
                self.last_event
            },
            provider_started_unix_millis: self.provider_started_unix_millis,
            provider_request: self
                .provider_request
                .and_then(PersistedProviderRequest::into_domain),
            attempt_base: super::super::progress::AttemptBase::default(),
        }
    }

    pub(super) fn provider_request_is_valid(&self) -> bool {
        match (&self.provider_request, self.provider_started_unix_millis) {
            (Some(request), Some(_)) => request.is_valid(),
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

impl PersistedProviderRequest {
    fn from_domain(value: ProviderRequestIdentity) -> Self {
        Self {
            role: value.role,
            turn: value.turn,
            attempt: value.attempt,
            request_id_digest: value.request_id_digest,
            request_fingerprint: value.request_fingerprint,
            provider_profile_id: value.provider_profile_id,
            provider_profile_revision: value.provider_profile_revision,
            provider_name_digest: value.provider_name_digest,
            native_session_digest: value.native_session_digest,
            provider_selection_digest: value.provider_selection_digest,
        }
    }

    fn into_domain(self) -> Option<ProviderRequestIdentity> {
        if !self.is_valid() {
            return None;
        }
        Some(ProviderRequestIdentity {
            role: self.role,
            turn: self.turn,
            attempt: self.attempt,
            request_id_digest: self.request_id_digest,
            request_fingerprint: self.request_fingerprint,
            provider_profile_id: self.provider_profile_id,
            provider_profile_revision: self.provider_profile_revision,
            provider_name_digest: self.provider_name_digest,
            native_session_digest: self.native_session_digest,
            provider_selection_digest: self.provider_selection_digest,
        })
    }

    fn is_valid(&self) -> bool {
        matches!(self.role, 1..=3)
            && self.attempt > 0
            && self.provider_profile_revision > 0
            && self.provider_profile_id != [0; 16]
    }
}

impl PersistedResourceMeasurement {
    fn from_domain(value: ResourceMeasurement) -> Self {
        let coverage = PersistedResourceCoverage::from_domain(value.coverage());
        match value.status() {
            ResourceMeasurementStatus::Measured => value.measured_value().map_or_else(
                || Self::Unavailable {
                    coverage,
                    cause: PersistedResourceCause::WorkerUnavailable,
                },
                |value| Self::Measured { value, coverage },
            ),
            ResourceMeasurementStatus::Partial => Self::Partial {
                value: value.value(),
                coverage,
                cause: PersistedResourceCause::from_domain(
                    value.cause().unwrap_or(ResourceObservationCause::WorkerUnavailable),
                ),
            },
            ResourceMeasurementStatus::Unavailable => Self::Unavailable {
                coverage,
                cause: PersistedResourceCause::from_domain(
                    value.cause().unwrap_or(ResourceObservationCause::WorkerUnavailable),
                ),
            },
        }
    }

    fn into_domain(self) -> ResourceMeasurement {
        match self {
            Self::Measured { value, coverage } => {
                ResourceMeasurement::measured(value, coverage.into_domain())
            }
            Self::Partial { value, coverage, cause } => ResourceMeasurement::partial(
                value,
                coverage.into_domain(),
                cause.into_domain(),
            ),
            Self::Unavailable { coverage, cause } => {
                ResourceMeasurement::unavailable_with_coverage(
                    cause.into_domain(),
                    coverage.into_domain(),
                )
            }
        }
    }
}

impl PersistedResourceCoverage {
    fn from_domain(value: ResourceObservationCoverage) -> Self {
        Self {
            entries_observed: value.entries_observed(),
            directories_completed: value.directories_completed(),
            items_unavailable: value.items_unavailable(),
            directories_pending: value.directories_pending(),
        }
    }

    fn into_domain(self) -> ResourceObservationCoverage {
        ResourceObservationCoverage::new(
            self.entries_observed,
            self.directories_completed,
            self.items_unavailable,
            self.directories_pending,
        )
    }
}

impl PersistedResourceCause {
    fn from_domain(value: ResourceObservationCause) -> Self {
        match value {
            ResourceObservationCause::NotObserved => Self::NotObserved,
            ResourceObservationCause::LegacyUnspecified => Self::LegacyUnspecified,
            ResourceObservationCause::BaselineEstablishedAfterStart => {
                Self::BaselineEstablishedAfterStart
            }
            ResourceObservationCause::NotAuthorized => Self::NotAuthorized,
            ResourceObservationCause::ScanInProgress => Self::ScanInProgress,
            ResourceObservationCause::Cancelled => Self::Cancelled,
            ResourceObservationCause::ArithmeticOverflow => Self::ArithmeticOverflow,
            ResourceObservationCause::WorkerUnavailable => Self::WorkerUnavailable,
            ResourceObservationCause::InvalidPlatformData => Self::InvalidPlatformData,
            ResourceObservationCause::PlatformCommandFailed { exit_code, signal } => {
                Self::PlatformCommandFailed { exit_code, signal }
            }
            ResourceObservationCause::Io { operation, kind, raw_os_error } => Self::Io {
                operation: PersistedResourceOperation::from_domain(operation),
                kind: PersistedResourceIoKind::from_domain(kind),
                raw_os_error,
            },
        }
    }

    fn into_domain(self) -> ResourceObservationCause {
        match self {
            Self::NotObserved => ResourceObservationCause::NotObserved,
            Self::LegacyUnspecified => ResourceObservationCause::LegacyUnspecified,
            Self::BaselineEstablishedAfterStart => {
                ResourceObservationCause::BaselineEstablishedAfterStart
            }
            Self::NotAuthorized => ResourceObservationCause::NotAuthorized,
            Self::ScanInProgress => ResourceObservationCause::ScanInProgress,
            Self::Cancelled => ResourceObservationCause::Cancelled,
            Self::ArithmeticOverflow => ResourceObservationCause::ArithmeticOverflow,
            Self::WorkerUnavailable => ResourceObservationCause::WorkerUnavailable,
            Self::InvalidPlatformData => ResourceObservationCause::InvalidPlatformData,
            Self::PlatformCommandFailed { exit_code, signal } => {
                ResourceObservationCause::PlatformCommandFailed { exit_code, signal }
            }
            Self::Io { operation, kind, raw_os_error } => ResourceObservationCause::Io {
                operation: operation.into_domain(),
                kind: kind.into_domain(),
                raw_os_error,
            },
        }
    }
}

impl PersistedResourceOperation {
    const fn from_domain(value: ResourceObservationOperation) -> Self {
        match value {
            ResourceObservationOperation::OpenDirectory => Self::OpenDirectory,
            ResourceObservationOperation::ReadDirectoryEntry => Self::ReadDirectoryEntry,
            ResourceObservationOperation::ReadMetadata => Self::ReadMetadata,
            ResourceObservationOperation::ReadProcessMemory => Self::ReadProcessMemory,
            ResourceObservationOperation::StartResourceObserver => {
                Self::StartResourceObserver
            }
            ResourceObservationOperation::StartProcessMemoryCommand => {
                Self::StartProcessMemoryCommand
            }
        }
    }

    const fn into_domain(self) -> ResourceObservationOperation {
        match self {
            Self::OpenDirectory => ResourceObservationOperation::OpenDirectory,
            Self::ReadDirectoryEntry => ResourceObservationOperation::ReadDirectoryEntry,
            Self::ReadMetadata => ResourceObservationOperation::ReadMetadata,
            Self::ReadProcessMemory => ResourceObservationOperation::ReadProcessMemory,
            Self::StartResourceObserver => {
                ResourceObservationOperation::StartResourceObserver
            }
            Self::StartProcessMemoryCommand => {
                ResourceObservationOperation::StartProcessMemoryCommand
            }
        }
    }
}

impl PersistedResourceIoKind {
    const fn from_domain(value: ResourceIoErrorKind) -> Self {
        match value {
            ResourceIoErrorKind::NotFound => Self::NotFound,
            ResourceIoErrorKind::PermissionDenied => Self::PermissionDenied,
            ResourceIoErrorKind::ConnectionRefused => Self::ConnectionRefused,
            ResourceIoErrorKind::ConnectionReset => Self::ConnectionReset,
            ResourceIoErrorKind::ConnectionAborted => Self::ConnectionAborted,
            ResourceIoErrorKind::NotConnected => Self::NotConnected,
            ResourceIoErrorKind::AddressInUse => Self::AddressInUse,
            ResourceIoErrorKind::AddressNotAvailable => Self::AddressNotAvailable,
            ResourceIoErrorKind::BrokenPipe => Self::BrokenPipe,
            ResourceIoErrorKind::AlreadyExists => Self::AlreadyExists,
            ResourceIoErrorKind::WouldBlock => Self::WouldBlock,
            ResourceIoErrorKind::InvalidInput => Self::InvalidInput,
            ResourceIoErrorKind::InvalidData => Self::InvalidData,
            ResourceIoErrorKind::TimedOut => Self::TimedOut,
            ResourceIoErrorKind::WriteZero => Self::WriteZero,
            ResourceIoErrorKind::Interrupted => Self::Interrupted,
            ResourceIoErrorKind::Unsupported => Self::Unsupported,
            ResourceIoErrorKind::UnexpectedEof => Self::UnexpectedEof,
            ResourceIoErrorKind::OutOfMemory => Self::OutOfMemory,
            ResourceIoErrorKind::Other => Self::Other,
        }
    }

    const fn into_domain(self) -> ResourceIoErrorKind {
        match self {
            Self::NotFound => ResourceIoErrorKind::NotFound,
            Self::PermissionDenied => ResourceIoErrorKind::PermissionDenied,
            Self::ConnectionRefused => ResourceIoErrorKind::ConnectionRefused,
            Self::ConnectionReset => ResourceIoErrorKind::ConnectionReset,
            Self::ConnectionAborted => ResourceIoErrorKind::ConnectionAborted,
            Self::NotConnected => ResourceIoErrorKind::NotConnected,
            Self::AddressInUse => ResourceIoErrorKind::AddressInUse,
            Self::AddressNotAvailable => ResourceIoErrorKind::AddressNotAvailable,
            Self::BrokenPipe => ResourceIoErrorKind::BrokenPipe,
            Self::AlreadyExists => ResourceIoErrorKind::AlreadyExists,
            Self::WouldBlock => ResourceIoErrorKind::WouldBlock,
            Self::InvalidInput => ResourceIoErrorKind::InvalidInput,
            Self::InvalidData => ResourceIoErrorKind::InvalidData,
            Self::TimedOut => ResourceIoErrorKind::TimedOut,
            Self::WriteZero => ResourceIoErrorKind::WriteZero,
            Self::Interrupted => ResourceIoErrorKind::Interrupted,
            Self::Unsupported => ResourceIoErrorKind::Unsupported,
            Self::UnexpectedEof => ResourceIoErrorKind::UnexpectedEof,
            Self::OutOfMemory => ResourceIoErrorKind::OutOfMemory,
            Self::Other => ResourceIoErrorKind::Other,
        }
    }
}

impl PersistedResourceTelemetry {
    fn into_domain(self) -> (ResourceMeasurement, ResourceMeasurement, ResourceMeasurement) {
        (
            self.workspace.into_domain(),
            self.workspace_growth.into_domain(),
            self.peak_rss.into_domain(),
        )
    }
}

fn legacy_resource_telemetry() -> (ResourceMeasurement, ResourceMeasurement, ResourceMeasurement) {
    let unavailable =
        ResourceMeasurement::unavailable(ResourceObservationCause::LegacyUnspecified);
    (unavailable, unavailable, unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_request() -> ProviderRequestIdentity {
        peritus_agent::DeveloperProviderRequestIdentity::from_parts(
            peritus_agent::DeveloperModelRole::Reviewer,
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
        .into()
    }

    #[test]
    fn active_provider_elapsed_time_survives_persistence_and_reconnection() {
        let progress = RunProgress {
            started_unix_millis: 10,
            last_effect_unix_millis: 20,
            provider_started_unix_millis: Some(30),
            provider_request: Some(provider_request()),
            last_event: "provider turn started".to_owned(),
            model_requests: 4,
            tool_calls: 2,
            ..RunProgress::default()
        };
        let restored = PersistedProgress::from_run(&progress).into_run();
        assert_eq!(restored.started_unix_millis, 10);
        assert_eq!(restored.last_effect_unix_millis, 20);
        assert_eq!(restored.provider_started_unix_millis, Some(30));
        assert_eq!(restored.provider_request, Some(provider_request()));
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
