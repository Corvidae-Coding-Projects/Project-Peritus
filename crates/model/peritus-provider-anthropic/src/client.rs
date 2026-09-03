//! Provider ownership, authenticated submission, and conservative retry execution.

use core::fmt;
use std::time::Instant;

use peritus_model_protocol::{
    FailureCategory, ModelEvent, ModelRequest, ProviderProfile, ResponseId,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialSource, Header, HeaderName, HttpHeaders, HttpMethod,
    HttpRequest, HttpResponse, HttpTransport, ModelProvider, OwnedModelStream,
    ProviderAvailability, ProviderCoreError, ProviderCoreErrorKind, ReqwestTransport, RetryAction,
    RetryFailure, RetryObservation, SubmissionState, validate_request_profile, wait_for_backoff,
};

use crate::config::AnthropicConfig;
use crate::error::{ambiguous_transport, status_failure, stream_failure};
use crate::stream::AnthropicStream;

/// Configured first-party Anthropic Messages provider.
pub struct AnthropicClient {
    config: AnthropicConfig,
    credentials: Box<dyn CredentialSource>,
    transport: Box<dyn HttpTransport>,
}

impl AnthropicClient {
    /// Owns one immutable adapter configuration, credential source, and hardened transport.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration failure when the Reqwest/Rustls client cannot be
    /// constructed. Redirects, ambient proxies, and implicit HTTP retries remain disabled.
    pub fn new(
        config: AnthropicConfig,
        credentials: Box<dyn CredentialSource>,
    ) -> Result<Self, ProviderCoreError> {
        let transport = Box::new(ReqwestTransport::new(config.http_limits())?);
        Ok(Self { config, credentials, transport })
    }

    #[cfg(test)]
    pub(crate) fn with_transport(
        config: AnthropicConfig,
        credentials: Box<dyn CredentialSource>,
        transport: Box<dyn HttpTransport>,
    ) -> Self {
        Self { config, credentials, transport }
    }

    /// Returns this instance's exact immutable profile.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        self.config.profile()
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one auditable loop owns submission state, retries, and ambiguity classification"
    )]
    fn start_inner(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            validate_request_profile(self.config.profile(), &request)?;
            let body = crate::request::encode(&request, &self.config)?;
            let endpoint = self.config.endpoint().with_path("/v1/messages")?;
            let started = Instant::now();
            let mut attempt = 1_u32;
            let mut cumulative_bytes = 0_u64;
            loop {
                if cancellation.is_cancelled() {
                    return Err(ProviderCoreError::cancelled("anthropic_start"));
                }
                cumulative_bytes = cumulative_bytes
                    .checked_add(u64::try_from(body.len()).map_err(|_| {
                        ProviderCoreError::limit_exceeded(
                            "anthropic_start",
                            "request byte count cannot be represented",
                        )
                    })?)
                    .ok_or_else(|| {
                        ProviderCoreError::limit_exceeded(
                            "anthropic_start",
                            "cumulative request byte count overflowed",
                        )
                    })?;
                let request = self.http_request(endpoint.clone(), body.clone())?;
                let response = match self.transport.send(request, &cancellation).await {
                    Ok(response) => response,
                    Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                        return Err(error);
                    }
                    Err(error) if error.kind() == ProviderCoreErrorKind::Connect => {
                        let observation = RetryObservation::new(
                            attempt,
                            started.elapsed(),
                            cumulative_bytes,
                            SubmissionState::NotSent,
                            RetryFailure::Connect,
                        );
                        let plan = self.config.retry_policy().plan(observation)?;
                        if plan.action() != RetryAction::RetryFresh {
                            return Err(error);
                        }
                        wait_for_backoff(plan, &cancellation).await?;
                        attempt = next_attempt(attempt)?;
                        continue;
                    }
                    Err(_error) => {
                        let failure =
                            ambiguous_transport(self.config.profile().provider().clone())?;
                        let stream =
                            AnthropicStream::terminal(ModelEvent::ResponseFailed(failure))?;
                        return Ok(OwnedModelStream::new(stream, cancellation));
                    }
                };
                if response.status().as_u16() == 200 {
                    if !is_event_stream(response.headers()) {
                        let failure = stream_failure(
                            self.config.profile().provider().clone(),
                            FailureCategory::MalformedPayload,
                            false,
                            "anthropic.http.content_type",
                        )?;
                        let stream =
                            AnthropicStream::terminal(ModelEvent::ResponseFailed(failure))?;
                        return Ok(OwnedModelStream::new(stream, cancellation));
                    }
                    let stream = AnthropicStream::new(
                        response,
                        self.config.profile().provider().clone(),
                        self.config.framing_limits(),
                    )?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
                let status = response.status().as_u16();
                let retry_after = retry_after_millis(response.headers());
                let error_facts = drain_error(response, &cancellation).await?;
                let failure = status_failure(
                    self.config.profile().provider().clone(),
                    status,
                    retry_after,
                    error_facts.quota_hint,
                    error_facts.response_id,
                )?;
                let retry_failure = match status {
                    429 if !error_facts.quota_hint => RetryFailure::RateLimited,
                    409 | 500 | 504 | 529 => RetryFailure::Server,
                    401..=403 => RetryFailure::Authentication,
                    _ => RetryFailure::InvalidRequest,
                };
                let elapsed = started.elapsed();
                let mut observation = RetryObservation::new(
                    attempt,
                    elapsed,
                    cumulative_bytes,
                    SubmissionState::Rejected,
                    retry_failure,
                );
                if let Some(delay) = retry_after.map(std::time::Duration::from_millis) {
                    observation = observation.with_retry_after(delay);
                }
                let plan = self.config.retry_policy().plan(observation)?;
                if plan.action() != RetryAction::RetryFresh {
                    let stream = AnthropicStream::terminal(ModelEvent::ResponseFailed(failure))?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
                wait_for_backoff(plan, &cancellation).await?;
                attempt = next_attempt(attempt)?;
            }
        })
    }

    fn http_request(
        &self,
        endpoint: peritus_provider_core::Endpoint,
        body: Vec<u8>,
    ) -> Result<HttpRequest, ProviderCoreError> {
        let credential = self.credentials.resolve(self.config.credential())?;
        let mut headers = vec![
            credential.into_header(name("x-api-key")?, None)?,
            Header::new(name("anthropic-version")?, b"2023-06-01".to_vec())?,
            Header::new(name("content-type")?, b"application/json".to_vec())?,
            Header::new(name("accept")?, b"text/event-stream".to_vec())?,
        ];
        if let Some(beta) = self.config.beta_header() {
            headers.push(Header::new(name("anthropic-beta")?, beta)?);
        }
        let headers = HttpHeaders::new(headers, self.config.http_limits())?;
        HttpRequest::new(HttpMethod::Post, endpoint, headers, body, self.config.http_limits())
    }
}

impl ModelProvider for AnthropicClient {
    fn profile(&self) -> &ProviderProfile {
        self.profile()
    }

    fn availability(&self) -> ProviderAvailability {
        if self.credentials.resolve(self.config.credential()).is_ok() {
            ProviderAvailability::CredentialPresent
        } else {
            ProviderAvailability::Unavailable
        }
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.start_inner(request, cancellation)
    }
}

impl fmt::Debug for AnthropicClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicClient")
            .field("config", &self.config)
            .field("credentials", &"[private credential source]")
            .field("transport", &"[private HTTP transport]")
            .finish()
    }
}

async fn drain_error(
    response: HttpResponse,
    cancellation: &CancellationToken,
) -> Result<HttpErrorFacts, ProviderCoreError> {
    let (_status, headers, mut body) = response.into_parts();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next(cancellation).await? {
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
    let kind = value
        .as_ref()
        .and_then(|value| value.pointer("/error/type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let response_id = header_text(&headers, "request-id")
        .or_else(|| header_text(&headers, "x-request-id"))
        .or_else(|| {
            value
                .as_ref()
                .and_then(|value| value.get("request_id"))
                .and_then(serde_json::Value::as_str)
        })
        .and_then(|value| ResponseId::new(value.to_owned()).ok());
    Ok(HttpErrorFacts {
        quota_hint: matches!(kind, "billing_error" | "credit_balance_error" | "quota_error"),
        response_id,
    })
}

struct HttpErrorFacts {
    quota_hint: bool,
    response_id: Option<ResponseId>,
}

fn header_text<'a>(headers: &'a HttpHeaders, name: &str) -> Option<&'a str> {
    let bytes = headers.first(name)?.nonsensitive_bytes()?;
    core::str::from_utf8(bytes).ok()
}

fn retry_after_millis(headers: &HttpHeaders) -> Option<u64> {
    let value = headers.first("retry-after")?.nonsensitive_bytes()?;
    decimal_seconds_to_millis(core::str::from_utf8(value).ok()?)
}

fn decimal_seconds_to_millis(value: &str) -> Option<u64> {
    let (seconds, fraction) =
        value.split_once('.').map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    let seconds = seconds.parse::<u64>().ok()?.checked_mul(1_000)?;
    let fraction = match fraction {
        None => 0,
        Some(fraction)
            if !fraction.is_empty()
                && fraction.len() <= 3
                && fraction.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let parsed = fraction.parse::<u64>().ok()?;
            parsed.checked_mul(10_u64.checked_pow(u32::try_from(3 - fraction.len()).ok()?)?)?
        }
        Some(_) => return None,
    };
    seconds.checked_add(fraction)
}

fn is_event_stream(headers: &HttpHeaders) -> bool {
    let Some(bytes) = headers.first("content-type").and_then(|value| value.nonsensitive_bytes())
    else {
        return false;
    };
    core::str::from_utf8(bytes)
        .ok()
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("text/event-stream"))
}

fn name(value: &'static str) -> Result<HeaderName, ProviderCoreError> {
    HeaderName::new(value.to_owned())
}

fn next_attempt(attempt: u32) -> Result<u32, ProviderCoreError> {
    attempt.checked_add(1).ok_or_else(|| {
        ProviderCoreError::limit_exceeded("anthropic_retry", "retry attempt count overflowed")
    })
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
