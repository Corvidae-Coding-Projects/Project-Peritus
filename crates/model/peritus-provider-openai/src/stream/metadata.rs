//! Bounded response-header observations and HTTP error classification.

use peritus_model_protocol::{
    FailureCategory, ModelEvent, OutcomeCertainty, ProviderName, RateLimitDimension,
    RateLimitObservation, RateLimitWindow, ResetTime, ResponseId, Retryability, TransportPhase,
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
        let request_id = text_header(headers, "x-request-id", 512)?;
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
) -> Result<ModelEvent, ProviderCoreError> {
    let value: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let code = value
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("code"))
        .and_then(serde_json::Value::as_str);
    let status_number = status.as_u16();
    let (category, certainty, retryability, diagnostic) = classify(status_number, code);
    let retry_after = retry_after(headers)?;
    let response_id =
        text_header(headers, "x-request-id", 512)?.and_then(|value| ResponseId::new(value).ok());
    let failure = error::failure(
        provider,
        category,
        TransportPhase::ReadingBody,
        certainty,
        retryability,
        Some(status_number),
        response_id,
        retry_after,
        diagnostic,
    )?;
    Ok(ModelEvent::ResponseFailed(failure))
}

pub fn retry_directive(
    status: StatusCode,
    headers: &HttpHeaders,
    body: &[u8],
) -> Result<Option<(RetryFailure, Option<u64>)>, ProviderCoreError> {
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
    failure.map(|failure| Ok((failure, retry_after(headers)?))).transpose()
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

fn retry_after(headers: &HttpHeaders) -> Result<Option<u64>, ProviderCoreError> {
    if let Some(value) = text_header(headers, "retry-after-ms", 64)? {
        return Ok(value.parse::<u64>().ok().filter(|millis| *millis <= 86_400_000));
    }
    let Some(value) = text_header(headers, "retry-after", 64)? else {
        return Ok(None);
    };
    let seconds = value.parse::<u64>().ok().filter(|seconds| *seconds <= 86_400);
    Ok(seconds.and_then(|seconds| seconds.checked_mul(1_000)))
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
