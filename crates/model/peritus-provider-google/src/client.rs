//! Provider ownership, authenticated stable-v1 submission, and conservative retry execution.

use core::fmt;
use std::time::Instant;

use peritus_model_protocol::{FailureCategory, ModelEvent, ModelRequest, ProviderProfile};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialSource, Header, HeaderName, HttpHeaders, HttpMethod,
    HttpRequest, HttpTransport, ModelProvider, OwnedModelStream,
    ProviderAvailability, ProviderCoreError, ProviderCoreErrorKind, ReqwestTransport, RetryAction,
    RetryFailure, RetryObservation, SubmissionState, cancel_first, validate_request_profile,
    admit_request_bytes, can_admit_request_bytes, wait_for_backoff,
};

use crate::config::GoogleConfig;
use crate::error::{ambiguous_transport, status_failure, stream_failure};
use crate::rejection::GoogleRejectionStream;
use crate::stream::GoogleStream;

/// Configured first-party Google Gemini stable-v1 provider.
pub struct GoogleClient {
    config: GoogleConfig,
    credentials: std::sync::Arc<dyn CredentialSource>,
    transport: std::sync::Arc<dyn HttpTransport>,
    catalog: tokio::sync::Mutex<peritus_provider_core::catalog::HttpCatalogDiscovery>,
    projection: tokio::sync::Mutex<Option<crate::request::Projection>>,
}

impl GoogleClient {
    /// Owns one immutable adapter configuration, credential source, and hardened transport.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration failure when the Reqwest/Rustls transport cannot be
    /// constructed. Redirects, ambient proxies, and implicit HTTP retries remain disabled.
    pub fn new(
        config: GoogleConfig,
        credentials: Box<dyn CredentialSource>,
    ) -> Result<Self, ProviderCoreError> {
        let transport: Box<dyn HttpTransport> =
            Box::new(ReqwestTransport::new(config.http_limits())?);
        let catalog = tokio::sync::Mutex::new(config.catalog_discovery());
        Ok(Self {
            config,
            credentials: credentials.into(),
            transport: transport.into(),
            catalog,
            projection: tokio::sync::Mutex::new(None),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_transport(
        config: GoogleConfig,
        credentials: Box<dyn CredentialSource>,
        transport: Box<dyn HttpTransport>,
    ) -> Self {
        let catalog = tokio::sync::Mutex::new(config.catalog_discovery());
        Self {
            config,
            credentials: credentials.into(),
            transport: transport.into(),
            catalog,
            projection: tokio::sync::Mutex::new(None),
        }
    }

    /// Returns this instance's exact immutable profile.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        self.config.profile()
    }

    async fn project_request(
        &self,
        request: &ModelRequest,
        cancellation: &CancellationToken,
    ) -> Result<crate::request::EncodedRequest, ProviderCoreError> {
        let Some(mut cached) = cancel_first(cancellation, self.projection.lock()).await else {
            return Err(ProviderCoreError::cancelled("google_projection"));
        };
        let matches = cached
            .as_ref()
            .map(|projection| projection.matches(request))
            .transpose()?;
        if matches != Some(true) {
            *cached = Some(crate::request::Projection::new(request, &self.config)?);
        }
        loop {
            if cancellation.is_cancelled() {
                return Err(ProviderCoreError::cancelled("google_projection"));
            }
            let result = cached
                .as_mut()
                .ok_or_else(|| {
                    ProviderCoreError::invalid_request(
                        "google_projection",
                        "Google projection state disappeared",
                    )
                })?
                .advance(request);
            let complete = match result {
                Ok(complete) => complete,
                Err(error) => {
                    *cached = None;
                    return Err(error);
                }
            };
            if complete {
                if cancellation.is_cancelled() {
                    return Err(ProviderCoreError::cancelled("google_projection"));
                }
                let finish = cached
                    .as_mut()
                    .ok_or_else(|| {
                        ProviderCoreError::invalid_request(
                            "google_projection",
                            "Google projection state disappeared",
                        )
                    })?
                    .finish_encoding(request, &self.config, Some(cancellation));
                if let Err(error) = finish {
                    if error.kind() == ProviderCoreErrorKind::Cancelled {
                        return Err(error);
                    }
                    *cached = None;
                    return Err(error);
                }
                if cancellation.is_cancelled() {
                    return Err(ProviderCoreError::cancelled("google_projection"));
                }
                let encoded = cached
                    .as_mut()
                    .ok_or_else(|| {
                        ProviderCoreError::invalid_request(
                            "google_projection",
                            "Google projection state disappeared",
                        )
                    })?
                    .take_encoded()?;
                *cached = None;
                return Ok(encoded);
            }
            cooperate().await;
        }
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
            self.validate_request(&request)?;
            let encoded = self.project_request(&request, &cancellation).await?;
            let dialect = request.dialect();
            let protocol_limits = request.protocol_limits();
            let started = Instant::now();
            let mut attempt = 1_u32;
            let mut cumulative_bytes = 0_u64;
            let request_bytes = encoded.body.len();
            let retry_policy = self.config.retry_policy();
            loop {
                if cancellation.is_cancelled() {
                    return Err(ProviderCoreError::cancelled("google_start"));
                }
                cumulative_bytes = admit_request_bytes(
                    cumulative_bytes,
                    request_bytes,
                    retry_policy.max_cumulative_bytes(),
                )?;
                let request = self.http_request(encoded.endpoint.clone(), encoded.body.clone())?;
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
                        let plan = retry_policy.plan(observation)?;
                        if plan.action() != RetryAction::RetryFresh
                            || !can_admit_request_bytes(
                                cumulative_bytes,
                                request_bytes,
                                retry_policy.max_cumulative_bytes(),
                            )?
                        {
                            return Err(error);
                        }
                        wait_for_backoff(plan, &cancellation).await?;
                        attempt = next_attempt(attempt)?;
                        continue;
                    }
                    Err(_error) => {
                        let failure =
                            ambiguous_transport(self.config.profile().provider().clone())?;
                        let stream = GoogleStream::terminal(ModelEvent::ResponseFailed(failure))?;
                        return Ok(OwnedModelStream::new(stream, cancellation));
                    }
                };
                if response.status().as_u16() == 200 {
                    if !is_event_stream(response.headers()) {
                        let failure = stream_failure(
                            self.config.profile().provider().clone(),
                            FailureCategory::MalformedPayload,
                            false,
                            "google.http.content_type",
                        )?;
                        let stream = GoogleStream::terminal(ModelEvent::ResponseFailed(failure))?;
                        return Ok(OwnedModelStream::new(stream, cancellation));
                    }
                    let stream = GoogleStream::new(
                        response,
                        self.config.profile().provider().clone(),
                        dialect,
                        encoded.structured,
                        encoded.tool_controls.clone(),
                        self.config.framing_limits(),
                        protocol_limits,
                    )?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
                let (status, headers, body) = response.into_parts();
                let retry_after = crate::metadata::retry_after(&headers, protocol_limits)?;
                let response_identity = crate::metadata::response_identity(&headers);
                let mut failure = status_failure(
                    self.config.profile().provider().clone(),
                    status.as_u16(),
                    retry_after.delay_millis,
                    false,
                    response_identity.response_id,
                )?;
                if let Some(observation) = retry_after.observation {
                    failure = failure.with_retry_after_observation(observation).map_err(|_| {
                        ProviderCoreError::malformed_stream(
                            "google_retry_after",
                            "Google retry-after classification was inconsistent",
                        )
                    })?;
                }
                if let Some(observation) = response_identity.observation {
                    failure = failure.with_optional_observation(observation);
                }
                let stream = GoogleRejectionStream::new(
                    body,
                    failure,
                    self.config.http_limits().max_response_body_bytes(),
                );
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
        })
    }

    fn http_request(
        &self,
        endpoint: peritus_provider_core::Endpoint,
        body: Vec<u8>,
    ) -> Result<HttpRequest, ProviderCoreError> {
        let credential = self.credentials.resolve(self.config.credential())?;
        let headers = vec![
            credential.into_header(name("x-goog-api-key")?, None)?,
            Header::new(name("content-type")?, b"application/json".to_vec())?,
            Header::new(name("accept")?, b"text/event-stream".to_vec())?,
        ];
        let headers = HttpHeaders::new(headers, self.config.http_limits())?;
        HttpRequest::new(HttpMethod::Post, endpoint, headers, body, self.config.http_limits())
    }
}

impl ModelProvider for GoogleClient {
    fn validate_request(&self, request: &ModelRequest) -> Result<(), ProviderCoreError> {
        validate_request_profile(self.config.profile(), request)?;
        crate::request::validate(request, &self.config)
    }

    fn supports_reasoning_effort(&self, effort: peritus_model_protocol::ReasoningEffort) -> bool {
        self.profile()
            .capabilities()
            .supports(peritus_model_protocol::Capability::ReasoningControls)
            && !matches!(
                effort,
                peritus_model_protocol::ReasoningEffort::XHigh
                    | peritus_model_protocol::ReasoningEffort::Max
                    | peritus_model_protocol::ReasoningEffort::Ultra
            )
    }

    fn discover_models<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<
        'a,
        Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, ProviderCoreError>,
    > {
        Box::pin(async move {
            use peritus_provider_core::catalog::CatalogProgress;
            let mut discovery = self.catalog.lock().await;
            loop {
                let progress = discovery
                    .resume_page(
                        self.transport.as_ref(),
                        &|| {
                    let credential = self.credentials.resolve(self.config.credential())?;
                    let headers = vec![credential.into_header(name("x-goog-api-key")?, None)?];

                    HttpHeaders::new(headers, self.config.http_limits())
                        },
                        self.config.http_limits(),
                        cancellation,
                    )
                    .await?;
                if progress == CatalogProgress::Complete {
                    let completed = core::mem::replace(
                        &mut *discovery,
                        self.config.catalog_discovery(),
                    );
                    return completed.into_models();
                }
            }
        })
    }

    fn select_model(
        &self,
        model: peritus_model_protocol::ModelName,
    ) -> Result<std::sync::Arc<dyn ModelProvider>, ProviderCoreError> {
        let profile = peritus_provider_core::catalog::selected_profile(self.profile(), model)?;
        let config = self.config.clone().with_selected_profile(profile)?;
        Ok(std::sync::Arc::new(Self {
            catalog: tokio::sync::Mutex::new(config.catalog_discovery()),
            projection: tokio::sync::Mutex::new(None),
            config,
            credentials: std::sync::Arc::clone(&self.credentials),
            transport: std::sync::Arc::clone(&self.transport),
        }))
    }

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

impl fmt::Debug for GoogleClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleClient")
            .field("config", &self.config)
            .field("credentials", &"[private credential source]")
            .field("transport", &"[private HTTP transport]")
            .field("catalog", &"[resumable model catalog]")
            .field("projection", &"[resumable request projection]")
            .finish()
    }
}

async fn cooperate() {
    let mut yielded = false;
    std::future::poll_fn(move |context| {
        if yielded {
            core::task::Poll::Ready(())
        } else {
            yielded = true;
            context.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    })
    .await;
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
        ProviderCoreError::limit_exceeded("google_retry", "retry attempt count overflowed")
    })
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
