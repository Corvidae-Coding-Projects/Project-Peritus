use std::time::{SystemTime, UNIX_EPOCH};

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, FailureCategory, JsonBounds, ModelEvent, OutcomeCertainty,
    ProviderExtension, ProviderName, RateLimitObservation, RateLimitWindow, ResetTime, ResponseId,
    RetryAfterObservation, RetryAfterParseStatus, RetryAfterUnit, Retryability, TransportPhase,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, RetryFailure, StatusCode};

use crate::{
    CompatibleConfig, CompatibleResetUnit, CompatibleResponseHeaders, CompatibleRetryStatuses,
    error,
};

pub(super) fn success(
    config: &CompatibleConfig,
    headers: &HttpHeaders,
) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let mut events = Vec::new();
    let mappings = config.response_headers();
    if let Some(name) = mappings.request_id()
        && let Some(value) = text_header(headers, name.as_str(), headers.byte_count())?
    {
        events.push(provider_text_event("compatible.request_id", &value)?);
    }
    let mut windows = Vec::new();
    for mapping in mappings.rate_limits() {
        let limit = integer_header(headers, mapping.limit().as_str())?;
        let remaining = integer_header(headers, mapping.remaining().as_str())?;
        let reset = match integer_header(headers, mapping.reset().as_str())? {
            Some(value) => Some(match mapping.reset_unit() {
                CompatibleResetUnit::Milliseconds => value,
                CompatibleResetUnit::Seconds => value
                    .checked_mul(1_000)
                    .ok_or_else(|| error::limit("compatible rate-limit reset overflowed"))?,
            }),
            None => None,
        }
        .map(ResetTime::AfterMillis);
        if limit.is_some() || remaining.is_some() || reset.is_some() {
            let window = RateLimitWindow::new(mapping.dimension().clone(), limit, remaining, reset)
                .map_err(|_| error::malformed("compatible rate-limit headers were inconsistent"))?;
            windows.push(window);
        }
    }
    if !windows.is_empty() {
        let observation = RateLimitObservation::new(windows)
            .map_err(|_| error::malformed("compatible rate-limit observation was invalid"))?;
        events.push(ModelEvent::RateLimit(observation));
    }
    Ok(events)
}

pub(super) fn http_failure(
    config: &CompatibleConfig,
    status: StatusCode,
    headers: &HttpHeaders,
    provider: &ProviderName,
    retry_after: &CompatibleRetryAfter,
) -> Result<ModelEvent, ProviderCoreError> {
    let status_number = status.as_u16();
    let (category, certainty, retryability, code) =
        classify(status_number, config.retry_statuses());
    let request_id = mapped_request_id(config.response_headers(), headers)?;
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
    Ok(ModelEvent::ResponseFailed(failure))
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

fn mapped_request_id(
    mappings: &CompatibleResponseHeaders,
    headers: &HttpHeaders,
) -> Result<Option<ResponseId>, ProviderCoreError> {
    let Some(name) = mappings.request_id() else { return Ok(None) };
    text_header(headers, name.as_str(), headers.byte_count())?
        .map(|value| {
            ResponseId::new(value)
                .map_err(|_| error::malformed("mapped compatible request identity was invalid"))
        })
        .transpose()
}

fn integer_header(headers: &HttpHeaders, name: &str) -> Result<Option<u64>, ProviderCoreError> {
    let Some(value) = text_header(headers, name, 64)? else { return Ok(None) };
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| error::malformed("mapped compatible integer header was malformed"))
}

fn text_header(
    headers: &HttpHeaders,
    name: &str,
    maximum: usize,
) -> Result<Option<String>, ProviderCoreError> {
    let Some(value) = headers.first(name) else { return Ok(None) };
    let Some(bytes) = value.nonsensitive_bytes() else { return Ok(None) };
    if bytes.len() > maximum {
        return Err(error::limit("mapped compatible response header exceeded its bound"));
    }
    core::str::from_utf8(bytes)
        .map(str::to_owned)
        .map(Some)
        .map_err(|_| error::malformed("mapped compatible response header was not UTF-8"))
}

fn provider_text_event(name: &str, value: &str) -> Result<ModelEvent, ProviderCoreError> {
    let name = ExtensionName::new(name.to_owned())
        .map_err(|_| error::malformed("static compatible extension name was invalid"))?;
    let encoded = serde_json::to_string(value)
        .map_err(|_| error::malformed("compatible observation serialization failed"))?;
    let value = CanonicalJson::parse(
        &encoded,
        JsonBounds::value(peritus_model_protocol::ProtocolLimits::PRODUCTION),
    )
    .map_err(|_| error::malformed("compatible provider observation exceeded bounds"))?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
}
