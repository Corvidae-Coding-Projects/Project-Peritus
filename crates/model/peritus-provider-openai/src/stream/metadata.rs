//! Bounded response-header observations and HTTP error classification.

use std::time::{SystemTime, UNIX_EPOCH};

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, FailureCategory, JsonBounds, ModelEvent, ModelFailure,
    OptionalObservation, OptionalObservationKind, OptionalObservationStatus, OutcomeCertainty,
    ProtocolLimits, ProviderExtension, ProviderName, RateLimitDimension, RateLimitObservation,
    RateLimitWindow, ResetTime, ResponseId, RetryAfterObservation, RetryAfterParseStatus,
    RetryAfterUnit, Retryability, TransportPhase,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, RetryFailure, StatusCode};

use crate::error;

pub struct ResponseMetadata {
    events: Vec<ModelEvent>,
}

impl ResponseMetadata {
    pub const fn empty() -> Self {
        Self { events: Vec::new() }
    }

    pub fn parse(headers: &HttpHeaders, limits: ProtocolLimits) -> Self {
        let mut events = Vec::new();
        match mapped_request_id(headers) {
            MappedRequestId::Missing => {}
            MappedRequestId::Accepted(value) => events.push(provider_text_event(
                "openai.request_id",
                value.expose_for_wire(),
                limits,
            )),
            MappedRequestId::Rejected(observation) => {
                events.push(ModelEvent::OptionalObservation(observation));
            }
        }
        let mut windows = Vec::new();
        let mut rate_limit_evidence = Vec::new();
        add_window(
            headers,
            "requests",
            RateLimitDimension::Requests,
            &mut windows,
            &mut events,
            &mut rate_limit_evidence,
        );
        add_window(
            headers,
            "tokens",
            RateLimitDimension::TotalTokens,
            &mut windows,
            &mut events,
            &mut rate_limit_evidence,
        );
        add_window(
            headers,
            "project-tokens",
            RateLimitDimension::TotalTokens,
            &mut windows,
            &mut events,
            &mut rate_limit_evidence,
        );
        if !windows.is_empty() {
            match RateLimitObservation::new(windows) {
                Ok(observation) => events.push(ModelEvent::RateLimit(observation)),
                Err(_) => events.push(ModelEvent::OptionalObservation(OptionalObservation::new(
                    OptionalObservationKind::RateLimitWindow,
                    OptionalObservationStatus::Inconsistent,
                    &rate_limit_evidence,
                ))),
            }
        }
        Self { events }
    }

    pub fn take_events(&mut self) -> Vec<ModelEvent> {
        std::mem::take(&mut self.events)
    }
}

pub fn http_failure(
    status: StatusCode,
    headers: &HttpHeaders,
    provider: &ProviderName,
    retry_after: &OpenAiRetryAfter,
) -> Result<ModelFailure, ProviderCoreError> {
    let status_number = status.as_u16();
    let (category, certainty, retryability, diagnostic) = classify(status_number);
    let (response_id, rejected_request_id) = match mapped_request_id(headers) {
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
        response_id,
        retry_after.delay_millis,
        diagnostic,
    )?;
    if let Some(observation) = retry_after.observation.clone() {
        failure = failure.with_retry_after_observation(observation).map_err(|_| {
            error::malformed("OpenAI retry-after observation was inconsistent")
        })?;
    }
    if let Some(observation) = rejected_request_id {
        failure = failure.with_optional_observation(observation);
    }
    Ok(failure)
}

pub fn retry_directive(
    status: StatusCode,
    retry_after: &OpenAiRetryAfter,
) -> Option<(RetryFailure, Option<u64>)> {
    if !retry_after.local_scheduling_available {
        return None;
    }
    let failure = match status.as_u16() {
        429 => Some(RetryFailure::RateLimited),
        500..=599 => Some(RetryFailure::Server),
        _ => None,
    };
    failure.map(|failure| (failure, retry_after.delay_millis))
}

#[derive(Clone)]
pub struct OpenAiRetryAfter {
    delay_millis: Option<u64>,
    observation: Option<RetryAfterObservation>,
    local_scheduling_available: bool,
}

pub fn retry_after(
    headers: &HttpHeaders,
    limits: peritus_model_protocol::ProtocolLimits,
) -> Result<OpenAiRetryAfter, ProviderCoreError> {
    if let Some(value) = headers.first("retry-after-ms") {
        let Some(raw_value) = value.nonsensitive_bytes() else {
            return Ok(empty_retry_after());
        };
        return parse_millisecond_retry_after(raw_value, limits);
    }
    let Some(value) = headers.first("retry-after") else {
        return Ok(empty_retry_after());
    };
    let Some(raw_value) = value.nonsensitive_bytes() else {
        return Ok(empty_retry_after());
    };
    parse_standard_retry_after(raw_value, limits)
}

const fn empty_retry_after() -> OpenAiRetryAfter {
    OpenAiRetryAfter {
        delay_millis: None,
        observation: None,
        local_scheduling_available: true,
    }
}

fn parse_millisecond_retry_after(
    raw_value: &[u8],
    limits: peritus_model_protocol::ProtocolLimits,
) -> Result<OpenAiRetryAfter, ProviderCoreError> {
    let Ok(text) = core::str::from_utf8(raw_value) else {
        return unschedulable_retry_after(
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
            limits,
        );
    };
    let value = text.trim_matches(|character| matches!(character, ' ' | '\t'));
    if !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && let Ok(delay_millis) = value.parse::<u64>()
    {
        return Ok(OpenAiRetryAfter {
            delay_millis: Some(delay_millis),
            observation: None,
            local_scheduling_available: true,
        });
    }
    unschedulable_retry_after(
        raw_value,
        RetryAfterUnit::Unsupported,
        RetryAfterParseStatus::Invalid,
        limits,
    )
}

fn parse_standard_retry_after(
    raw_value: &[u8],
    limits: peritus_model_protocol::ProtocolLimits,
) -> Result<OpenAiRetryAfter, ProviderCoreError> {
    let Ok(text) = core::str::from_utf8(raw_value) else {
        return unschedulable_retry_after(
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
            limits,
        );
    };
    let value = text.trim_matches(|character| matches!(character, ' ' | '\t'));
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        let Ok(seconds) = value.parse::<u64>() else {
            return unschedulable_retry_after(
                raw_value,
                RetryAfterUnit::DeltaSeconds,
                RetryAfterParseStatus::Unrepresentable,
                limits,
            );
        };
        let Some(delay_millis) = seconds.checked_mul(1_000) else {
            return unschedulable_retry_after(
                raw_value,
                RetryAfterUnit::DeltaSeconds,
                RetryAfterParseStatus::Unrepresentable,
                limits,
            );
        };
        let legacy_accepted = raw_value.len() <= 64 && value == text && seconds <= 86_400;
        let observation = (!legacy_accepted)
            .then(|| {
                retry_after_observation(
                    raw_value,
                    RetryAfterUnit::DeltaSeconds,
                    RetryAfterParseStatus::Parsed,
                    None,
                    limits,
                )
            })
            .transpose()?;
        return Ok(OpenAiRetryAfter {
            delay_millis: Some(delay_millis),
            observation,
            local_scheduling_available: true,
        });
    }
    let Ok(eligible) = httpdate::parse_http_date(value) else {
        return unschedulable_retry_after(
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
            limits,
        );
    };
    parsed_http_date(raw_value, eligible, limits)
}

fn parsed_http_date(
    raw_value: &[u8],
    eligible: SystemTime,
    limits: peritus_model_protocol::ProtocolLimits,
) -> Result<OpenAiRetryAfter, ProviderCoreError> {
    let eligible_unix_millis = match eligible.duration_since(UNIX_EPOCH) {
        Ok(duration) => match u64::try_from(duration.as_millis()) {
            Ok(value) => value,
            Err(_) => {
                return unschedulable_retry_after(
                    raw_value,
                    RetryAfterUnit::HttpDate,
                    RetryAfterParseStatus::Unrepresentable,
                    limits,
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
            raw_value,
            RetryAfterUnit::HttpDate,
            RetryAfterParseStatus::Unrepresentable,
            limits,
        );
    };
    let parse_status = if eligible_unix_millis <= observed_unix_millis {
        RetryAfterParseStatus::AlreadyEligible
    } else {
        RetryAfterParseStatus::Parsed
    };
    let delay_millis = eligible_unix_millis.saturating_sub(observed_unix_millis);
    Ok(OpenAiRetryAfter {
        delay_millis: Some(delay_millis),
        observation: Some(retry_after_observation(
            raw_value,
            RetryAfterUnit::HttpDate,
            parse_status,
            Some(eligible_unix_millis),
            limits,
        )?),
        local_scheduling_available: true,
    })
}

fn unschedulable_retry_after(
    raw_value: &[u8],
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
    limits: peritus_model_protocol::ProtocolLimits,
) -> Result<OpenAiRetryAfter, ProviderCoreError> {
    Ok(OpenAiRetryAfter {
        delay_millis: None,
        observation: Some(retry_after_observation(
            raw_value,
            unit,
            parse_status,
            None,
            limits,
        )?),
        local_scheduling_available: false,
    })
}

fn retry_after_observation(
    raw_value: &[u8],
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
    eligible_unix_millis: Option<u64>,
    limits: peritus_model_protocol::ProtocolLimits,
) -> Result<RetryAfterObservation, ProviderCoreError> {
    RetryAfterObservation::new(
        raw_value,
        unit,
        parse_status,
        eligible_unix_millis,
        limits,
    )
    .map_err(|_| error::malformed("OpenAI retry-after observation was invalid"))
}

const fn classify(
    status: u16,
) -> (FailureCategory, OutcomeCertainty, Retryability, &'static str) {
    match status {
        400 | 422 => (
            FailureCategory::InvalidRequest,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "openai.http.invalid_request",
        ),
        401 => (
            FailureCategory::Authentication,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "openai.http.authentication",
        ),
        403 => (
            FailureCategory::Permission,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "openai.http.permission",
        ),
        404 => (
            FailureCategory::NotFound,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "openai.http.not_found",
        ),
        409 => (
            FailureCategory::Provider,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "openai.http.conflict",
        ),
        429 => (
            FailureCategory::RateLimited,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::SafeNewRequest,
            "openai.http.rate_limited",
        ),
        500..=599 => (
            FailureCategory::TransientProvider,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::SafeNewRequest,
            "openai.http.transient",
        ),
        _ => (
            FailureCategory::Provider,
            OutcomeCertainty::MaybeAccepted,
            Retryability::Never,
            "openai.http.provider",
        ),
    }
}

fn add_window(
    headers: &HttpHeaders,
    suffix: &str,
    dimension: RateLimitDimension,
    windows: &mut Vec<RateLimitWindow>,
    events: &mut Vec<ModelEvent>,
    aggregate_evidence: &mut Vec<u8>,
) {
    let mut trusted = true;
    let (limit, limit_raw) = integer_value(
        integer_header(
            headers,
            &format!("x-ratelimit-limit-{suffix}"),
            OptionalObservationKind::RateLimitLimit,
        ),
        events,
        &mut trusted,
    );
    let (remaining, remaining_raw) = integer_value(
        integer_header(
            headers,
            &format!("x-ratelimit-remaining-{suffix}"),
            OptionalObservationKind::RateLimitRemaining,
        ),
        events,
        &mut trusted,
    );
    let (reset_millis, reset_raw) = duration_value(
        duration_header(headers, &format!("x-ratelimit-reset-{suffix}")),
        events,
        &mut trusted,
    );
    if !trusted {
        return;
    }
    let reset = reset_millis.map(ResetTime::AfterMillis);
    if limit.is_some() || remaining.is_some() || reset.is_some() {
        let evidence = rate_limit_evidence([limit_raw, remaining_raw, reset_raw]);
        aggregate_evidence.extend_from_slice(&evidence);
        match RateLimitWindow::new(dimension, limit, remaining, reset) {
            Ok(window) => windows.push(window),
            Err(_) => events.push(ModelEvent::OptionalObservation(OptionalObservation::new(
                OptionalObservationKind::RateLimitWindow,
                OptionalObservationStatus::Inconsistent,
                &evidence,
            ))),
        }
    }
}

const MAX_REQUEST_ID_BYTES: usize = 512;
const MAX_NUMERIC_HEADER_BYTES: usize = 64;

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

enum MappedDuration<'a> {
    Missing,
    Accepted { millis: u64, raw: &'a [u8] },
    Rejected(OptionalObservation),
}

fn mapped_request_id(headers: &HttpHeaders) -> MappedRequestId {
    let Some(value) = headers.first("x-request-id") else { return MappedRequestId::Missing };
    let Some(bytes) = value.nonsensitive_bytes() else { return MappedRequestId::Missing };
    if bytes.len() > MAX_REQUEST_ID_BYTES {
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
    if bytes.len() > MAX_NUMERIC_HEADER_BYTES {
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

fn duration_header<'a>(headers: &'a HttpHeaders, name: &str) -> MappedDuration<'a> {
    let Some(value) = headers.first(name) else { return MappedDuration::Missing };
    let Some(bytes) = value.nonsensitive_bytes() else { return MappedDuration::Missing };
    if bytes.len() > MAX_NUMERIC_HEADER_BYTES {
        return MappedDuration::Rejected(OptionalObservation::new(
            OptionalObservationKind::RateLimitReset,
            OptionalObservationStatus::ExceededBound,
            bytes,
        ));
    }
    let Ok(text) = core::str::from_utf8(bytes) else {
        return MappedDuration::Rejected(OptionalObservation::new(
            OptionalObservationKind::RateLimitReset,
            OptionalObservationStatus::InvalidEncoding,
            bytes,
        ));
    };
    match duration_millis(text) {
        Ok(millis) => MappedDuration::Accepted { millis, raw: bytes },
        Err(status) => MappedDuration::Rejected(OptionalObservation::new(
            OptionalObservationKind::RateLimitReset,
            status,
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

fn duration_value<'a>(
    value: MappedDuration<'a>,
    events: &mut Vec<ModelEvent>,
    trusted: &mut bool,
) -> (Option<u64>, Option<&'a [u8]>) {
    match value {
        MappedDuration::Missing => (None, None),
        MappedDuration::Accepted { millis, raw } => (Some(millis), Some(raw)),
        MappedDuration::Rejected(observation) => {
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

fn provider_text_event(name: &str, value: &str, limits: ProtocolLimits) -> ModelEvent {
    let encoded = serde_json::Value::String(value.to_owned()).to_string();
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

fn duration_millis(value: &str) -> Result<u64, OptionalObservationStatus> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(OptionalObservationStatus::InvalidValue);
    }
    let mut index = 0_usize;
    let mut total = 0_u128;
    while index < bytes.len() {
        let start = index;
        let mut whole = 0_u128;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            whole = whole
                .checked_mul(10)
                .and_then(|value| value.checked_add(u128::from(bytes[index] - b'0')))
                .ok_or(OptionalObservationStatus::Unrepresentable)?;
            index += 1;
        }
        if index == start {
            return Err(OptionalObservationStatus::InvalidValue);
        }
        let mut fraction = 0_u128;
        let mut fraction_scale = 1_u128;
        if bytes.get(index) == Some(&b'.') {
            index += 1;
            let fraction_start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                fraction = fraction
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(u128::from(bytes[index] - b'0')))
                    .ok_or(OptionalObservationStatus::Unrepresentable)?;
                fraction_scale = fraction_scale
                    .checked_mul(10)
                    .ok_or(OptionalObservationStatus::Unrepresentable)?;
                index += 1;
            }
            if index == fraction_start {
                return Err(OptionalObservationStatus::InvalidValue);
            }
        }
        let unit_millis = if bytes[index..].starts_with(b"ms") {
            index += 2;
            1_u128
        } else if bytes.get(index) == Some(&b'h') {
            index += 1;
            3_600_000_u128
        } else if bytes.get(index) == Some(&b'm') {
            index += 1;
            60_000_u128
        } else if bytes.get(index) == Some(&b's') {
            index += 1;
            1_000_u128
        } else {
            return Err(OptionalObservationStatus::InvalidValue);
        };
        let whole_millis = whole
            .checked_mul(unit_millis)
            .ok_or(OptionalObservationStatus::Unrepresentable)?;
        let fraction_numerator = fraction
            .checked_mul(unit_millis)
            .ok_or(OptionalObservationStatus::Unrepresentable)?;
        if fraction_numerator % fraction_scale != 0 {
            return Err(OptionalObservationStatus::Unrepresentable);
        }
        total = total
            .checked_add(whole_millis)
            .and_then(|value| value.checked_add(fraction_numerator / fraction_scale))
            .ok_or(OptionalObservationStatus::Unrepresentable)?;
    }
    u64::try_from(total).map_err(|_| OptionalObservationStatus::Unrepresentable)
}
