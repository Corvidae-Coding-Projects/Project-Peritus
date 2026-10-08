//! Bounded response-header observations and HTTP error classification.

use std::time::{SystemTime, UNIX_EPOCH};

use peritus_model_protocol::{
    FailureCategory, ModelEvent, OutcomeCertainty, ProviderName, RateLimitDimension,
    RateLimitObservation, RateLimitWindow, ResetTime, ResponseId, RetryAfterObservation,
    RetryAfterParseStatus, RetryAfterUnit, Retryability, TransportPhase,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, RetryFailure, StatusCode};

use crate::error;

pub struct ResponseMetadata {
    request_id: Option<String>,
    rate_limit: Option<RateLimitObservation>,
}

impl ResponseMetadata {
    pub const fn empty() -> Self {
        Self { request_id: None, rate_limit: None }
    }

    pub fn parse(headers: &HttpHeaders) -> Result<Self, ProviderCoreError> {
        let request_id = text_header(headers, "x-request-id", headers.byte_count())?;
        let mut windows = Vec::new();
        add_window(headers, "requests", RateLimitDimension::Requests, &mut windows)?;
        add_window(headers, "tokens", RateLimitDimension::TotalTokens, &mut windows)?;
        add_window(headers, "project-tokens", RateLimitDimension::TotalTokens, &mut windows)?;
        let rate_limit = if windows.is_empty() {
            None
        } else {
            Some(
                RateLimitObservation::new(windows)
                    .map_err(|_| error::malformed("OpenAI rate-limit headers were inconsistent"))?,
            )
        };
        Ok(Self { request_id, rate_limit })
    }

    pub const fn take_request_id(&mut self) -> Option<String> {
        self.request_id.take()
    }

    pub const fn take_rate_limit(&mut self) -> Option<RateLimitObservation> {
        self.rate_limit.take()
    }
}

pub fn http_failure(
    status: StatusCode,
    headers: &HttpHeaders,
    body: &[u8],
    provider: &ProviderName,
    retry_after: &OpenAiRetryAfter,
) -> Result<ModelEvent, ProviderCoreError> {
    let value: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let code = value
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("code"))
        .and_then(serde_json::Value::as_str);
    let status_number = status.as_u16();
    let (category, certainty, retryability, diagnostic) = classify(status_number, code);
    let response_id = text_header(headers, "x-request-id", headers.byte_count())?
        .and_then(|value| ResponseId::new(value).ok());
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
    Ok(ModelEvent::ResponseFailed(failure))
}

pub fn retry_directive(
    status: StatusCode,
    body: &[u8],
    retry_after: &OpenAiRetryAfter,
) -> Option<(RetryFailure, Option<u64>)> {
    if !retry_after.local_scheduling_available {
        return None;
    }
    let code = serde_json::from_slice::<serde_json::Value>(body).ok().and_then(|value| {
        value
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    });
    let failure = match status.as_u16() {
        429 if !quota_code(code.as_deref()) => Some(RetryFailure::RateLimited),
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

fn classify(
    status: u16,
    code: Option<&str>,
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
        429 if quota_code(code) => (
            FailureCategory::QuotaExhausted,
            OutcomeCertainty::DefinitelyNotAccepted,
            Retryability::Never,
            "openai.http.quota",
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

fn quota_code(code: Option<&str>) -> bool {
    matches!(
        code,
        Some(
            "credit_balance_exhausted"
                | "organization_spend_limit_exceeded"
                | "project_spend_limit_exceeded"
                | "organization_usage_limit_exceeded"
                | "insufficient_quota"
        )
    )
}

fn add_window(
    headers: &HttpHeaders,
    suffix: &str,
    dimension: RateLimitDimension,
    windows: &mut Vec<RateLimitWindow>,
) -> Result<(), ProviderCoreError> {
    let limit = integer_header(headers, &format!("x-ratelimit-limit-{suffix}"))?;
    let remaining = integer_header(headers, &format!("x-ratelimit-remaining-{suffix}"))?;
    let reset = text_header(headers, &format!("x-ratelimit-reset-{suffix}"), 64)?
        .and_then(|value| duration_millis(&value))
        .map(ResetTime::AfterMillis);
    if limit.is_some() || remaining.is_some() || reset.is_some() {
        windows.push(
            RateLimitWindow::new(dimension, limit, remaining, reset)
                .map_err(|_| error::malformed("OpenAI rate-limit window was inconsistent"))?,
        );
    }
    Ok(())
}

fn integer_header(headers: &HttpHeaders, name: &str) -> Result<Option<u64>, ProviderCoreError> {
    Ok(text_header(headers, name, 64)?.and_then(|value| value.parse::<u64>().ok()))
}

fn text_header(
    headers: &HttpHeaders,
    name: &str,
    maximum: usize,
) -> Result<Option<String>, ProviderCoreError> {
    let Some(value) = headers.first(name) else { return Ok(None) };
    let Some(bytes) = value.nonsensitive_bytes() else { return Ok(None) };
    if bytes.len() > maximum {
        return Err(error::limit("OpenAI response header exceeds its field bound"));
    }
    let text = core::str::from_utf8(bytes)
        .map_err(|_| error::malformed("OpenAI response header is not UTF-8"))?;
    Ok(Some(text.to_owned()))
}

fn duration_millis(value: &str) -> Option<u64> {
    let mut total = 0_u64;
    let mut digits = String::new();
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        let number = digits.parse::<u64>().ok()?;
        digits.clear();
        let multiplier = match character {
            'h' => 3_600_000,
            'm' if chars.peek() == Some(&'s') => {
                chars.next();
                1
            }
            'm' => 60_000,
            's' => 1_000,
            _ => return None,
        };
        total = total.checked_add(number.checked_mul(multiplier)?)?;
        if total > 7 * 24 * 60 * 60 * 1_000 {
            return None;
        }
    }
    digits.is_empty().then_some(total)
}
