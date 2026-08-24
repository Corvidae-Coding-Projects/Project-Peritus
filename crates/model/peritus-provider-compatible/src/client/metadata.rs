use peritus_model_protocol::{
    CanonicalJson, ExtensionName, FailureCategory, JsonBounds, ModelEvent, OutcomeCertainty,
    ProviderExtension, ProviderName, RateLimitObservation, RateLimitWindow, ResetTime, ResponseId,
    Retryability, TransportPhase,
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
        && let Some(value) = text_header(headers, name.as_str(), 512)?
    {
        events.push(provider_text_event("compatible.request_id", &value)?);
    }
    if let Some(mapping) = mappings.rate_limit() {
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
            let observation = RateLimitObservation::new(vec![window])
                .map_err(|_| error::malformed("compatible rate-limit observation was invalid"))?;
            events.push(ModelEvent::RateLimit(observation));
        }
    }
    Ok(events)
}

pub(super) fn http_failure(
    config: &CompatibleConfig,
    status: StatusCode,
    headers: &HttpHeaders,
    provider: &ProviderName,
) -> Result<ModelEvent, ProviderCoreError> {
    let status_number = status.as_u16();
    let (category, certainty, retryability, code) =
        classify(status_number, config.retry_statuses());
    let request_id = mapped_request_id(config.response_headers(), headers)?;
    let failure = error::failure(
        provider,
        category,
        TransportPhase::ReadingBody,
        certainty,
        retryability,
        Some(status_number),
        request_id,
        retry_after(headers)?,
        code,
    )?;
    Ok(ModelEvent::ResponseFailed(failure))
}

pub(super) fn retry_directive(
    config: &CompatibleConfig,
    status: StatusCode,
    headers: &HttpHeaders,
) -> Result<Option<(RetryFailure, Option<u64>)>, ProviderCoreError> {
    let failure = match status.as_u16() {
        429 if config.retry_statuses().rate_limited() => Some(RetryFailure::RateLimited),
        500..=599 if config.retry_statuses().server_errors() => Some(RetryFailure::Server),
        _ => None,
    };
    failure.map(|failure| Ok((failure, retry_after(headers)?))).transpose()
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
    text_header(headers, name.as_str(), 512)?
        .map(|value| {
            ResponseId::new(value)
                .map_err(|_| error::malformed("mapped compatible request identity was invalid"))
        })
        .transpose()
}

fn retry_after(headers: &HttpHeaders) -> Result<Option<u64>, ProviderCoreError> {
    let Some(value) = text_header(headers, "retry-after", 64)? else { return Ok(None) };
    Ok(value
        .parse::<u64>()
        .ok()
        .filter(|seconds| *seconds <= 86_400)
        .and_then(|seconds| seconds.checked_mul(1_000)))
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
