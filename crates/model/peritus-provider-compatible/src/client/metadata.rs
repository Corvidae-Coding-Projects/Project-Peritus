use std::time::{SystemTime, UNIX_EPOCH};

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, FailureCategory, JsonBounds, ModelEvent, ModelFailure,
    OptionalObservation, OptionalObservationKind, OptionalObservationStatus, OutcomeCertainty,
    ProviderExtension, ProviderName, RateLimitObservation, RateLimitWindow, ResetTime, ResponseId,
    RetryAfterObservation, RetryAfterParseStatus, RetryAfterUnit, Retryability, TransportPhase,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, RetryFailure, StatusCode};
use serde_json::Value;

use crate::{
    CompatibleConfig, CompatibleResetUnit, CompatibleResponseHeaders, CompatibleRetryStatuses,
    error,
};

pub(super) fn success(
    config: &CompatibleConfig,
    headers: &HttpHeaders,
) -> Vec<ModelEvent> {
    let mut events = Vec::new();
    let mappings = config.response_headers();
    match mapped_request_id(mappings, headers) {
        MappedRequestId::Missing => {}
        MappedRequestId::Accepted(value) => events.push(provider_text_event(
            "compatible.request_id",
            value.expose_for_wire(),
            config.protocol_limits(),
        )),
        MappedRequestId::Rejected(observation) => {
            events.push(ModelEvent::OptionalObservation(observation));
        }
    }
    let mut windows = Vec::new();
    for mapping in mappings.rate_limits() {
        let mut trusted = true;
        let (limit, limit_raw) = integer_value(
            integer_header(
                headers,
                mapping.limit().as_str(),
                OptionalObservationKind::RateLimitLimit,
            ),
            &mut events,
            &mut trusted,
        );
        let (remaining, remaining_raw) = integer_value(
            integer_header(
                headers,
                mapping.remaining().as_str(),
                OptionalObservationKind::RateLimitRemaining,
            ),
            &mut events,
            &mut trusted,
        );
        let (reset_value, reset_raw) = integer_value(
            integer_header(
                headers,
                mapping.reset().as_str(),
                OptionalObservationKind::RateLimitReset,
            ),
            &mut events,
            &mut trusted,
        );
        if !trusted {
            continue;
        }
        let reset = match reset_value {
            Some(value) => {
                let normalized = match mapping.reset_unit() {
                    CompatibleResetUnit::Milliseconds => Some(value),
                    CompatibleResetUnit::Seconds => value.checked_mul(1_000),
                };
                let Some(normalized) = normalized else {
                    events.push(ModelEvent::OptionalObservation(OptionalObservation::new(
                        OptionalObservationKind::RateLimitReset,
                        OptionalObservationStatus::Unrepresentable,
                        reset_raw.unwrap_or(&[]),
                    )));
                    continue;
                };
                Some(ResetTime::AfterMillis(normalized))
            }
            None => None,
        };
        if limit.is_some() || remaining.is_some() || reset.is_some() {
            match RateLimitWindow::new(mapping.dimension().clone(), limit, remaining, reset) {
                Ok(window) => windows.push(window),
                Err(_) => events.push(ModelEvent::OptionalObservation(
                    OptionalObservation::new(
                        OptionalObservationKind::RateLimitWindow,
                        OptionalObservationStatus::Inconsistent,
                        &rate_limit_evidence([limit_raw, remaining_raw, reset_raw]),
                    ),
                )),
            }
        }
    }
    if !windows.is_empty() {
        if let Ok(observation) = RateLimitObservation::new(windows) {
            events.push(ModelEvent::RateLimit(observation));
        }
    }
    events
}

pub(super) fn http_failure(
    config: &CompatibleConfig,
    status: StatusCode,
    headers: &HttpHeaders,
    provider: &ProviderName,
    retry_after: &CompatibleRetryAfter,
) -> Result<ModelFailure, ProviderCoreError> {
    let status_number = status.as_u16();
    let (category, certainty, retryability, code) =
        classify(status_number, config.retry_statuses());
    let mapped_request_id = mapped_request_id(config.response_headers(), headers);
    let (request_id, rejected_request_id) = match mapped_request_id {
        MappedRequestId::Missing => (None, None),
        MappedRequestId::Accepted(value) => (Some(value), None),
        MappedRequestId::Rejected(observation) => (None, Some(observation)),
    };
    let mut failure = error::failure(
        provider,
        category,
        TransportPhase::ReadingBody,
        certainty,
        retryability,
        Some(status_number),
        request_id,
        retry_after.delay_millis,
        code,
    )?;
    if let Some(observation) = retry_after.observation.clone() {
        failure = failure.with_retry_after_observation(observation).map_err(|_| {
            error::malformed("compatible retry-after observation was inconsistent")
        })?;
    }
    if let Some(observation) = rejected_request_id {
        failure = failure.with_optional_observation(observation);
    }
    Ok(failure)
}

pub(super) fn retry_directive(
    config: &CompatibleConfig,
    status: StatusCode,
    retry_after: &CompatibleRetryAfter,
) -> Option<(RetryFailure, Option<u64>)> {
    if !retry_after.local_scheduling_available {
        return None;
    }
    let failure = match status.as_u16() {
        429 if config.retry_statuses().rate_limited() => Some(RetryFailure::RateLimited),
        500..=599 if config.retry_statuses().server_errors() => Some(RetryFailure::Server),
        _ => None,
    };
    failure.map(|failure| (failure, retry_after.delay_millis))
}

#[derive(Clone)]
pub(super) struct CompatibleRetryAfter {
    delay_millis: Option<u64>,
    observation: Option<RetryAfterObservation>,
    local_scheduling_available: bool,
}

pub(super) fn retry_after(
    config: &CompatibleConfig,
    headers: &HttpHeaders,
) -> Result<CompatibleRetryAfter, ProviderCoreError> {
    let Some(value) = headers.first("retry-after") else {
        return Ok(CompatibleRetryAfter {
            delay_millis: None,
            observation: None,
            local_scheduling_available: true,
        });
    };
    let Some(raw_value) = value.nonsensitive_bytes() else {
        return Ok(CompatibleRetryAfter {
            delay_millis: None,
            observation: None,
            local_scheduling_available: true,
        });
    };
    parse_retry_after(config, raw_value)
}

fn parse_retry_after(
    config: &CompatibleConfig,
    raw_value: &[u8],
) -> Result<CompatibleRetryAfter, ProviderCoreError> {
    let Ok(text) = core::str::from_utf8(raw_value) else {
        return unschedulable_retry_after(
            config,
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
        );
    };
    let value = text.trim_matches(|character| matches!(character, ' ' | '\t'));
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        let Ok(seconds) = value.parse::<u64>() else {
            return unschedulable_retry_after(
                config,
                raw_value,
                RetryAfterUnit::DeltaSeconds,
                RetryAfterParseStatus::Unrepresentable,
            );
        };
        let Some(delay_millis) = seconds.checked_mul(1_000) else {
            return unschedulable_retry_after(
                config,
                raw_value,
                RetryAfterUnit::DeltaSeconds,
                RetryAfterParseStatus::Unrepresentable,
            );
        };
        let legacy_accepted = raw_value.len() <= 64 && value == text && seconds <= 86_400;
        let observation = (!legacy_accepted)
            .then(|| {
                retry_after_observation(
                    config,
                    raw_value,
                    RetryAfterUnit::DeltaSeconds,
                    RetryAfterParseStatus::Parsed,
                    None,
                )
            })
            .transpose()?;
        return Ok(CompatibleRetryAfter {
            delay_millis: Some(delay_millis),
            observation,
            local_scheduling_available: true,
        });
    }
    let Ok(eligible) = httpdate::parse_http_date(value) else {
        return unschedulable_retry_after(
            config,
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
        );
    };
    parsed_http_date(config, raw_value, eligible)
}

fn parsed_http_date(
    config: &CompatibleConfig,
    raw_value: &[u8],
    eligible: SystemTime,
) -> Result<CompatibleRetryAfter, ProviderCoreError> {
    let eligible_unix_millis = match eligible.duration_since(UNIX_EPOCH) {
        Ok(duration) => match u64::try_from(duration.as_millis()) {
            Ok(value) => value,
            Err(_) => {
                return unschedulable_retry_after(
                    config,
                    raw_value,
                    RetryAfterUnit::HttpDate,
                    RetryAfterParseStatus::Unrepresentable,
                );
            }
        },
        Err(_) => 0,
    };
    let Some(observed_unix_millis) = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
    else {
        return unschedulable_retry_after(
            config,
            raw_value,
            RetryAfterUnit::HttpDate,
            RetryAfterParseStatus::Unrepresentable,
        );
    };
    let parse_status = if eligible_unix_millis <= observed_unix_millis {
        RetryAfterParseStatus::AlreadyEligible
    } else {
        RetryAfterParseStatus::Parsed
    };
    let delay_millis = eligible_unix_millis.saturating_sub(observed_unix_millis);
    Ok(CompatibleRetryAfter {
        delay_millis: Some(delay_millis),
        observation: Some(retry_after_observation(
            config,
            raw_value,
            RetryAfterUnit::HttpDate,
            parse_status,
            Some(eligible_unix_millis),
        )?),
        local_scheduling_available: true,
    })
}

fn unschedulable_retry_after(
    config: &CompatibleConfig,
    raw_value: &[u8],
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
) -> Result<CompatibleRetryAfter, ProviderCoreError> {
    Ok(CompatibleRetryAfter {
        delay_millis: None,
        observation: Some(retry_after_observation(
            config,
            raw_value,
            unit,
            parse_status,
            None,
        )?),
        local_scheduling_available: false,
    })
}

fn retry_after_observation(
    config: &CompatibleConfig,
    raw_value: &[u8],
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
    eligible_unix_millis: Option<u64>,
) -> Result<RetryAfterObservation, ProviderCoreError> {
    RetryAfterObservation::new(
        raw_value,
        unit,
        parse_status,
        eligible_unix_millis,
        config.protocol_limits(),
    )
    .map_err(|_| error::malformed("compatible retry-after observation was invalid"))
}

const fn classify(
    status: u16,
    retry: CompatibleRetryStatuses,
) -> (FailureCategory, OutcomeCertainty, Retryability, &'static str) {
    match status {
        400 | 422 => (
            FailureCategory::InvalidRequest,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.invalid_request",
        ),
        401 => (
            FailureCategory::Authentication,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.authentication",
        ),
        402 => (
            FailureCategory::QuotaExhausted,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.quota_exhausted",
        ),
        403 => (
            FailureCategory::Permission,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.permission",
        ),
        404 => (
            FailureCategory::NotFound,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.not_found",
        ),
        409 => (
            FailureCategory::Provider,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.conflict",
        ),
        429 if retry.rate_limited() => (
            FailureCategory::RateLimited,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::SafeNewRequest,
            "compatible.http.rate_limited",
        ),
        429 => (
            FailureCategory::RateLimited,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.rate_limited",
        ),
        500..=599 if retry.server_errors() => (
            FailureCategory::TransientProvider,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::SafeNewRequest,
            "compatible.http.transient",
        ),
        _ => (
            FailureCategory::Provider,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "compatible.http.provider",
        ),
    }
}

const MAX_MAPPED_REQUEST_ID_BYTES: usize = 512;
const MAX_MAPPED_INTEGER_BYTES: usize = 64;

enum MappedRequestId {
    Missing,
    Accepted(ResponseId),
    Rejected(OptionalObservation),
}

enum MappedInteger<'a> {
    Missing,
    Accepted { value: u64, raw: &'a [u8] },
    Rejected(OptionalObservation),
}

fn mapped_request_id(
    mappings: &CompatibleResponseHeaders,
    headers: &HttpHeaders,
) -> MappedRequestId {
    let Some(name) = mappings.request_id() else { return MappedRequestId::Missing };
    let Some(value) = headers.first(name.as_str()) else { return MappedRequestId::Missing };
    let Some(bytes) = value.nonsensitive_bytes() else { return MappedRequestId::Missing };
    if bytes.len() > MAX_MAPPED_REQUEST_ID_BYTES {
        return MappedRequestId::Rejected(OptionalObservation::new(
            OptionalObservationKind::MappedRequestId,
            OptionalObservationStatus::ExceededBound,
            bytes,
        ));
    }
    let Ok(text) = core::str::from_utf8(bytes) else {
        return MappedRequestId::Rejected(OptionalObservation::new(
            OptionalObservationKind::MappedRequestId,
            OptionalObservationStatus::InvalidEncoding,
            bytes,
        ));
    };
    match ResponseId::new(text.to_owned()) {
        Ok(value) => MappedRequestId::Accepted(value),
        Err(_) => MappedRequestId::Rejected(OptionalObservation::new(
            OptionalObservationKind::MappedRequestId,
            OptionalObservationStatus::InvalidValue,
            bytes,
        )),
    }
}

fn integer_header<'a>(
    headers: &'a HttpHeaders,
    name: &str,
    kind: OptionalObservationKind,
) -> MappedInteger<'a> {
    let Some(value) = headers.first(name) else { return MappedInteger::Missing };
    let Some(bytes) = value.nonsensitive_bytes() else { return MappedInteger::Missing };
    if bytes.len() > MAX_MAPPED_INTEGER_BYTES {
        return MappedInteger::Rejected(OptionalObservation::new(
            kind,
            OptionalObservationStatus::ExceededBound,
            bytes,
        ));
    }
    let Ok(text) = core::str::from_utf8(bytes) else {
        return MappedInteger::Rejected(OptionalObservation::new(
            kind,
            OptionalObservationStatus::InvalidEncoding,
            bytes,
        ));
    };
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return MappedInteger::Rejected(OptionalObservation::new(
            kind,
            OptionalObservationStatus::InvalidValue,
            bytes,
        ));
    }
    match text.parse::<u64>() {
        Ok(value) => MappedInteger::Accepted { value, raw: bytes },
        Err(_) => MappedInteger::Rejected(OptionalObservation::new(
            kind,
            OptionalObservationStatus::Unrepresentable,
            bytes,
        )),
    }
}

fn integer_value<'a>(
    value: MappedInteger<'a>,
    events: &mut Vec<ModelEvent>,
    trusted: &mut bool,
) -> (Option<u64>, Option<&'a [u8]>) {
    match value {
        MappedInteger::Missing => (None, None),
        MappedInteger::Accepted { value, raw } => (Some(value), Some(raw)),
        MappedInteger::Rejected(observation) => {
            *trusted = false;
            events.push(ModelEvent::OptionalObservation(observation));
            (None, None)
        }
    }
}

fn rate_limit_evidence(values: [Option<&[u8]>; 3]) -> Vec<u8> {
    let mut evidence = Vec::new();
    for value in values {
        let value = value.unwrap_or(&[]);
        evidence.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
        evidence.extend_from_slice(value);
    }
    evidence
}

fn provider_text_event(
    name: &str,
    value: &str,
    limits: peritus_model_protocol::ProtocolLimits,
) -> ModelEvent {
    let encoded = Value::String(value.to_owned()).to_string();
    let Ok(value_json) = CanonicalJson::parse(&encoded, JsonBounds::extension(limits)) else {
        return ModelEvent::OptionalObservation(OptionalObservation::new(
            OptionalObservationKind::MappedRequestId,
            OptionalObservationStatus::ExceededBound,
            value.as_bytes(),
        ));
    };
    let Ok(name) = ExtensionName::new(name.to_owned()) else {
        return ModelEvent::OptionalObservation(OptionalObservation::new(
            OptionalObservationKind::MappedRequestId,
            OptionalObservationStatus::InvalidValue,
            value.as_bytes(),
        ));
    };
    ModelEvent::ProviderEvent(ProviderExtension::new(name, value_json))
}
