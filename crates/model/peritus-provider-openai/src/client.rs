//! Provider composition, credential timing, HTTP submission, and stream construction.

mod cancel;
mod response;

use core::fmt;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use peritus_model_protocol::{ModelRequest, ProviderProfile, ResponseId, StructuredOutput};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialSource, HttpTransport, ModelProvider, OwnedModelStream,
    ProviderCoreError, ProviderCoreErrorKind, ReqwestTransport, ResponseCancellationOutcome,
    RetryAction, RetryFailure, RetryObservation, SubmissionState, validate_request_profile,
    wait_for_backoff,
};

use crate::config::OpenAiConfig;
use crate::error;
use crate::request::{self, RequestPlan};
use crate::stream::{OpenAiStream, metadata};
use response::{add_request_bytes, ambiguous_failure, connection_failure, is_event_stream};

/// One first-party `OpenAI` Responses adapter bound to an immutable profile revision.
pub struct OpenAiProvider {
    config: OpenAiConfig,
    profile: ProviderProfile,
    credentials: Arc<dyn CredentialSource>,
    transport: Arc<dyn HttpTransport>,
    resumable_background: Arc<Mutex<BTreeSet<ResponseId>>>,
}

/// Conventional client name for the first-party `OpenAI` Responses provider.
pub type OpenAiClient = OpenAiProvider;

impl OpenAiProvider {
    /// Creates a production `OpenAI` adapter with the default Reqwest/Rustls transport.
    ///
    /// The credential source is retained by identity, but credentials are not resolved until a
    /// completely validated request is ready for immediate encoding and submission.
    ///
    /// # Errors
    ///
    /// Rejects a non-OpenAI, non-Responses, capability-inconsistent profile or transport setup.
    pub fn new(
        config: OpenAiConfig,
        profile: ProviderProfile,
        credentials: Arc<dyn CredentialSource>,
    ) -> Result<Self, ProviderCoreError> {
        crate::profile::validate(&profile)?;
        let transport = ReqwestTransport::new(config.http_limits())?;
        Ok(Self::compose(config, profile, credentials, Arc::new(transport)))
    }

    fn compose(
        config: OpenAiConfig,
        profile: ProviderProfile,
        credentials: Arc<dyn CredentialSource>,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self {
            config,
            profile,
            credentials,
            transport,
            resumable_background: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_transport(
        config: OpenAiConfig,
        profile: ProviderProfile,
        credentials: Arc<dyn CredentialSource>,
        transport: Arc<dyn HttpTransport>,
    ) -> Result<Self, ProviderCoreError> {
        crate::profile::validate(&profile)?;
        Ok(Self::compose(config, profile, credentials, transport))
    }

    #[cfg(test)]
    pub(crate) fn remember_background_for_test(
        &self,
        response_id: ResponseId,
    ) -> Result<(), ProviderCoreError> {
        self.resumable_background
            .lock()
            .map_err(|_| error::invalid("OpenAI continuation registry is unavailable"))?
            .insert(response_id);
        Ok(())
    }

    fn exact_resume_was_observed(&self, plan: &RequestPlan) -> Result<(), ProviderCoreError> {
        let RequestPlan::Resume { response_id, .. } = plan else { return Ok(()) };
        let registry = self
            .resumable_background
            .lock()
            .map_err(|_| error::invalid("OpenAI continuation registry is unavailable"))?;
        if !registry.contains(response_id) {
            return Err(error::invalid(
                "exact continuation is limited to background streams observed by this adapter",
            ));
        }
        drop(registry);
        Ok(())
    }
}

impl ModelProvider for OpenAiProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the retry loop keeps every submission-state transition visible in one place"
    )]
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            validate_request_profile(&self.profile, &request)?;
            let plan = request::plan(&request)?;
            self.exact_resume_was_observed(&plan)?;
            let started = Instant::now();
            let mut attempt = 1_u32;
            let mut cumulative_bytes = 0_u64;
            loop {
                if cancellation.is_cancelled() {
                    return Err(ProviderCoreError::cancelled("openai_start"));
                }
                let credential = self.credentials.resolve(self.config.credential())?;
                let http_request =
                    request::http_request(&self.config, &request, &plan, credential)?;
                cumulative_bytes = add_request_bytes(cumulative_bytes, http_request.body().len())?;
                let response = match self.transport.send(http_request, &cancellation).await {
                    Ok(response) => response,
                    Err(failure) if failure.kind() == ProviderCoreErrorKind::Cancelled => {
                        return Err(failure);
                    }
                    Err(failure) if failure.kind() == ProviderCoreErrorKind::Connect => {
                        let observation = RetryObservation::new(
                            attempt,
                            started.elapsed(),
                            cumulative_bytes,
                            SubmissionState::NotSent,
                            RetryFailure::Connect,
                        );
                        let retry = self.config.retry_policy().plan(observation)?;
                        if retry.action() != RetryAction::RetryFresh {
                            return connection_failure(&self.profile, &cancellation);
                        }
                        wait_for_backoff(retry, &cancellation).await?;
                        attempt = next_attempt(attempt)?;
                        continue;
                    }
                    Err(failure) if failure.kind() == ProviderCoreErrorKind::Transport => {
                        return ambiguous_failure(&self.profile, &cancellation);
                    }
                    Err(failure) => return Err(failure),
                };
                let (status, headers, mut body) = response.into_parts();
                if !status.is_success() {
                    let bytes = response::read_body(
                        &mut body,
                        &cancellation,
                        self.config.http_limits().max_response_body_bytes(),
                    )
                    .await?;
                    let directive = metadata::retry_directive(status, &headers, &bytes)?;
                    let event =
                        metadata::http_failure(status, &headers, &bytes, self.profile.provider())?;
                    if let Some((failure, retry_after)) = directive {
                        let mut observation = RetryObservation::new(
                            attempt,
                            started.elapsed(),
                            cumulative_bytes,
                            SubmissionState::Rejected,
                            failure,
                        );
                        if let Some(delay) = retry_after.map(Duration::from_millis) {
                            observation = observation.with_retry_after(delay);
                        }
                        let retry = self.config.retry_policy().plan(observation)?;
                        if retry.action() == RetryAction::RetryFresh {
                            wait_for_backoff(retry, &cancellation).await?;
                            attempt = next_attempt(attempt)?;
                            continue;
                        }
                    }
                    let stream = OpenAiStream::failure_stream(
                        self.profile.provider().clone(),
                        event,
                        peritus_codec::sha256(&bytes),
                    )?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
                if status.as_u16() != 200 || !is_event_stream(&headers) {
                    return response::failure_stream(
                        &self.profile,
                        &cancellation,
                        peritus_model_protocol::FailureCategory::MalformedPayload,
                        peritus_model_protocol::TransportPhase::ReadingBody,
                        peritus_model_protocol::OutcomeCertainty::MaybeAccepted,
                        peritus_model_protocol::Retryability::Never,
                        "openai.http.success_shape",
                    );
                }
                let metadata = metadata::ResponseMetadata::parse(&headers)?;
                let stream = OpenAiStream::new(
                    body,
                    self.config.framing_limits(),
                    self.profile.provider().clone(),
                    self.profile.model().clone(),
                    !matches!(request.options().output(), StructuredOutput::Text),
                    self.config.protocol_limits(),
                    metadata,
                    matches!(plan, RequestPlan::Create)
                        && request.options().persistence().background(),
                    Arc::clone(&self.resumable_background),
                );
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
        })
    }

    fn cancel_response<'a>(
        &'a self,
        response_id: &'a ResponseId,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ResponseCancellationOutcome, ProviderCoreError>> {
        cancel::cancel(self, response_id, cancellation)
    }
}

impl fmt::Debug for OpenAiProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiProvider")
            .field("config", &self.config)
            .field("profile", &self.profile)
            .field("credentials", &"[private credential source]")
            .field("transport", &"[private HTTP transport]")
            .finish_non_exhaustive()
    }
}

fn next_attempt(attempt: u32) -> Result<u32, ProviderCoreError> {
    attempt.checked_add(1).ok_or_else(|| {
        ProviderCoreError::limit_exceeded("openai_retry", "OpenAI retry count overflowed")
    })
}
