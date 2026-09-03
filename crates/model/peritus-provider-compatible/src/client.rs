//! Provider composition, credential timing, conservative retries, and stream ownership.

mod metadata;
mod response;

use core::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use peritus_model_protocol::{ModelRequest, ProviderProfile, StructuredOutput};
use peritus_provider_core::{
    BoxFuture, CancellationToken, CredentialSource, HttpTransport, ModelProvider, OwnedModelStream,
    ProviderAvailability, ProviderCoreError, ProviderCoreErrorKind, ReqwestTransport, RetryAction,
    RetryFailure, RetryObservation, SubmissionState, validate_request_profile, wait_for_backoff,
};

use crate::{CompatibleConfig, CompatibleProfile, request, stream::CompatibleStream};

/// Production compatible provider bound to one immutable reviewed profile revision.
pub struct CompatibleClient {
    config: CompatibleConfig,
    profile: CompatibleProfile,
    credentials: Arc<dyn CredentialSource>,
    transport: Arc<dyn HttpTransport>,
}

impl CompatibleClient {
    /// Creates a compatible adapter owning the default Reqwest/Rustls transport.
    ///
    /// Credentials remain behind their source and are resolved only after request/profile
    /// validation, immediately before encoding and submission.
    ///
    /// # Errors
    ///
    /// Returns an error when the bounded production transport cannot be constructed.
    pub fn new(
        config: CompatibleConfig,
        profile: CompatibleProfile,
        credentials: Arc<dyn CredentialSource>,
    ) -> Result<Self, ProviderCoreError> {
        let transport = ReqwestTransport::new(config.http_limits())?;
        Ok(Self::compose(config, profile, credentials, Arc::new(transport)))
    }

    fn compose(
        config: CompatibleConfig,
        profile: CompatibleProfile,
        credentials: Arc<dyn CredentialSource>,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self { config, profile, credentials, transport }
    }

    #[cfg(test)]
    pub(crate) fn with_transport(
        config: CompatibleConfig,
        profile: CompatibleProfile,
        credentials: Arc<dyn CredentialSource>,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self::compose(config, profile, credentials, transport)
    }

    /// Returns the separately validated dialect contract.
    #[must_use]
    pub const fn compatible_profile(&self) -> &CompatibleProfile {
        &self.profile
    }
}

impl ModelProvider for CompatibleClient {
    fn profile(&self) -> &ProviderProfile {
        self.profile.provider_profile()
    }

    fn availability(&self) -> ProviderAvailability {
        if self.credentials.resolve(self.config.auth().credential()).is_ok() {
            ProviderAvailability::CredentialPresent
        } else {
            ProviderAvailability::Unavailable
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the retry loop keeps each submission-state transition auditable"
    )]
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            validate_request_profile(self.profile.provider_profile(), &request)?;
            request::validate(&self.profile, &request)?;
            let started = Instant::now();
            let mut attempt = 1_u32;
            let mut cumulative_bytes = 0_u64;
            loop {
                if cancellation.is_cancelled() {
                    return Err(ProviderCoreError::cancelled("compatible_start"));
                }
                let credential = self.credentials.resolve(self.config.auth().credential())?;
                let http_request =
                    request::http_request(&self.config, &self.profile, &request, credential)?;
                cumulative_bytes =
                    response::add_request_bytes(cumulative_bytes, http_request.body().len())?;
                let http_response = match self.transport.send(http_request, &cancellation).await {
                    Ok(value) => value,
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
                        let directive = self.config.retry_policy().plan(observation)?;
                        if directive.action() != RetryAction::RetryFresh {
                            return response::failure_stream(
                                &self.profile,
                                &cancellation,
                                peritus_model_protocol::FailureCategory::Transport,
                                peritus_model_protocol::TransportPhase::Connecting,
                                peritus_model_protocol::OutcomeCertainty::DefinitelyNotAccepted,
                                peritus_model_protocol::Retryability::SafeNewRequest,
                                "compatible.connect.failed",
                            );
                        }
                        wait_for_backoff(directive, &cancellation).await?;
                        attempt = next_attempt(attempt)?;
                        continue;
                    }
                    Err(failure) if failure.kind() == ProviderCoreErrorKind::Transport => {
                        return response::failure_stream(
                            &self.profile,
                            &cancellation,
                            peritus_model_protocol::FailureCategory::AmbiguousAcceptance,
                            peritus_model_protocol::TransportPhase::SendingBody,
                            peritus_model_protocol::OutcomeCertainty::MaybeAccepted,
                            peritus_model_protocol::Retryability::CallerDecision,
                            "compatible.submission.ambiguous",
                        );
                    }
                    Err(failure) => return Err(failure),
                };
                let (status, headers, mut body) = http_response.into_parts();
                if !status.is_success() {
                    let bytes = response::read_body(
                        &mut body,
                        &cancellation,
                        self.config.http_limits().max_response_body_bytes(),
                    )
                    .await?;
                    let retry = metadata::retry_directive(&self.config, status, &headers)?;
                    let event = metadata::http_failure(
                        &self.config,
                        status,
                        &headers,
                        self.profile.provider_profile().provider(),
                    )?;
                    if let Some((failure, retry_after)) = retry {
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
                        let directive = self.config.retry_policy().plan(observation)?;
                        if directive.action() == RetryAction::RetryFresh {
                            wait_for_backoff(directive, &cancellation).await?;
                            attempt = next_attempt(attempt)?;
                            continue;
                        }
                    }
                    let stream = CompatibleStream::failure_stream(
                        self.profile.provider_profile().provider().clone(),
                        event,
                        peritus_codec::sha256(&bytes),
                    )?;
                    return Ok(OwnedModelStream::new(stream, cancellation));
                }
                if status.as_u16() != 200 || !response::is_event_stream(&headers) {
                    return response::failure_stream(
                        &self.profile,
                        &cancellation,
                        peritus_model_protocol::FailureCategory::MalformedPayload,
                        peritus_model_protocol::TransportPhase::ReadingBody,
                        peritus_model_protocol::OutcomeCertainty::MaybeAccepted,
                        peritus_model_protocol::Retryability::Never,
                        "compatible.http.success_shape",
                    );
                }
                let response_metadata = metadata::success(&self.config, &headers)?;
                let stream = CompatibleStream::new(
                    body,
                    self.config.framing_limits(),
                    self.profile.provider_profile().provider().clone(),
                    self.profile.provider_profile().model().clone(),
                    self.profile.provider_profile().dialect(),
                    !matches!(request.options().output(), StructuredOutput::Text),
                    request.negotiated().includes(peritus_model_protocol::Capability::ToolCalls),
                    request.negotiated().includes(peritus_model_protocol::Capability::UsageDetail),
                    self.config.protocol_limits(),
                    response_metadata,
                )?;
                return Ok(OwnedModelStream::new(stream, cancellation));
            }
        })
    }
}

impl fmt::Debug for CompatibleClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompatibleClient")
            .field("config", &self.config)
            .field("profile", &self.profile)
            .field("credentials", &"[private credential source]")
            .field("transport", &"[private HTTP transport]")
            .finish_non_exhaustive()
    }
}

fn next_attempt(value: u32) -> Result<u32, ProviderCoreError> {
    value.checked_add(1).ok_or_else(|| {
        ProviderCoreError::limit_exceeded("compatible_retry", "compatible retry count overflowed")
    })
}
