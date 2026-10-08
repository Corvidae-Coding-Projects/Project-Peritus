//! Provider composition, credential timing, HTTP submission, and stream construction.

mod cancel;
mod catalog;
mod response;

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use peritus_model_protocol::{
    Capability, ModelRequest, ProviderProfile, ResponseId, ResponseReducer, ResumeKind, StateMode,
    StructuredOutput,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ContinuationRestoreOutcome, CredentialSource, HttpTransport,
    ModelProvider, OwnedModelStream, PersistedContinuation, ProviderAvailability,
    ProviderCoreError, ProviderCoreErrorKind, ReqwestTransport, ResponseCancellationOutcome,
    RetryAction, RetryFailure, RetryObservation, SubmissionState, validate_request_profile,
    wait_for_backoff,
};

use crate::config::OpenAiConfig;
use crate::error;
use crate::request::{self, RequestPlan};
use crate::stream::{OpenAiResumeState, OpenAiStream, metadata};
use response::{add_request_bytes, ambiguous_failure, connection_failure, is_event_stream};

/// One first-party `OpenAI` Responses adapter bound to an immutable profile revision.
pub struct OpenAiProvider {
    config: OpenAiConfig,
    profile: ProviderProfile,
    credentials: Arc<dyn CredentialSource>,
    transport: Arc<dyn HttpTransport>,
    resumable_background: Arc<Mutex<BTreeSet<ResponseId>>>,
    restored_continuations: Arc<Mutex<BTreeMap<ResponseId, OpenAiResumeState>>>,
}

struct ResumeClaim {
    response_id: ResponseId,
    state: Option<OpenAiResumeState>,
    registry: Arc<Mutex<BTreeMap<ResponseId, OpenAiResumeState>>>,
}

impl ResumeClaim {
    fn consume(mut self) -> Result<OpenAiResumeState, ProviderCoreError> {
        self.state
            .take()
            .ok_or_else(|| error::invalid("OpenAI continuation checkpoint was already consumed"))
    }
}

impl Drop for ResumeClaim {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else { return };
        let Ok(mut registry) = self.registry.lock() else { return };
        registry.entry(self.response_id.clone()).or_insert(state);
    }
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
        if config.is_gateway() && profile.state_mode() != StateMode::StatelessReplay {
            return Err(error::invalid(
                "OpenCode gateway supports only stateless replay in this adapter",
            ));
        }
        let transport = ReqwestTransport::new(config.http_limits())?;
        Ok(Self::compose(config, profile, credentials, Arc::new(transport)))
    }

    fn compose(
        config: OpenAiConfig,
        profile: ProviderProfile,
        credentials: Arc<dyn CredentialSource>,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self::compose_shared(
            config,
            profile,
            credentials,
            transport,
            Arc::new(Mutex::new(BTreeSet::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
        )
    }

    fn compose_shared(
        config: OpenAiConfig,
        profile: ProviderProfile,
        credentials: Arc<dyn CredentialSource>,
        transport: Arc<dyn HttpTransport>,
        resumable_background: Arc<Mutex<BTreeSet<ResponseId>>>,
        restored_continuations: Arc<Mutex<BTreeMap<ResponseId, OpenAiResumeState>>>,
    ) -> Self {
        Self {
            config,
            profile,
            credentials,
            transport,
            resumable_background,
            restored_continuations,
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

    fn claim_exact_resume(
        &self,
        plan: &RequestPlan,
    ) -> Result<Option<ResumeClaim>, ProviderCoreError> {
        let RequestPlan::Resume { response_id, sequence } = plan else { return Ok(None) };
        let known = self
            .resumable_background
            .lock()
            .map_err(|_| error::invalid("OpenAI continuation registry is unavailable"))?;
        if !known.contains(response_id) {
            return Err(error::invalid(
                "exact continuation is limited to background streams observed by this adapter",
            ));
        }
        let mut registry = self
            .restored_continuations
            .lock()
            .map_err(|_| error::invalid("OpenAI restored continuation registry is unavailable"))?;
        if !registry.get(response_id).is_some_and(|state| {
            state.matches(response_id, *sequence, self.profile.model())
        }) {
            return Err(error::invalid(
                "exact continuation does not match a restored OpenAI decoder checkpoint",
            ));
        }
        let state = registry.remove(response_id).ok_or_else(|| {
            error::invalid("OpenAI restored continuation checkpoint is unavailable")
        })?;
        drop(registry);
        drop(known);
        Ok(Some(ResumeClaim {
            response_id: response_id.clone(),
            state: Some(state),
            registry: Arc::clone(&self.restored_continuations),
        }))
    }
}

impl ModelProvider for OpenAiProvider {
    fn supports_reasoning_effort(&self, _effort: peritus_model_protocol::ReasoningEffort) -> bool {
        self.profile().capabilities().supports(Capability::ReasoningControls)
    }

    fn discover_models<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<
        'a,
        Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, ProviderCoreError>,
    > {
        Box::pin(self.catalog(cancellation))
    }

    fn select_model(
        &self,
        model: peritus_model_protocol::ModelName,
    ) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        let profile = peritus_provider_core::catalog::selected_profile(&self.profile, model)?;
        crate::profile::validate(&profile)?;
        Ok(Arc::new(Self::compose_shared(
            self.config.clone(),
            profile,
            Arc::clone(&self.credentials),
            Arc::clone(&self.transport),
            Arc::clone(&self.resumable_background),
            Arc::clone(&self.restored_continuations),
        )))
    }

    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn availability(&self) -> ProviderAvailability {
        if self.credentials.resolve(self.config.credential()).is_ok() {
            ProviderAvailability::CredentialPresent
        } else {
            ProviderAvailability::Unavailable
        }
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
            let mut resume = self.claim_exact_resume(&plan)?;
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
                let structured_output =
                    !matches!(request.options().output(), StructuredOutput::Text);
                let stream = if let Some(claim) = resume.take() {
                    OpenAiStream::resume(
                        body,
                        self.config.framing_limits(),
                        self.profile.provider().clone(),
                        structured_output,
                        self.config.protocol_limits(),
                        metadata,
                        Arc::clone(&self.resumable_background),
                        claim.consume()?,
                    )
                } else {
                    OpenAiStream::new(
                        body,
                        self.config.framing_limits(),
                        self.profile.provider().clone(),
                        self.profile.model().clone(),
                        structured_output,
                        self.config.protocol_limits(),
                        metadata,
                        matches!(&plan, RequestPlan::Create)
                            && request.options().persistence().background(),
                        Arc::clone(&self.resumable_background),
                    )
                };
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

    fn restore_continuation<'a>(
        &'a self,
        persisted: &'a PersistedContinuation,
    ) -> BoxFuture<'a, Result<ContinuationRestoreOutcome, ProviderCoreError>> {
        Box::pin(async move {
            let exact_profile = persisted.profile_id() == self.profile.profile_id()
                && persisted.profile_revision() == self.profile.revision();
            let exact_resume = self.profile.state_mode() == StateMode::BackgroundResumable
                && self.profile.resume_kind() == ResumeKind::ExactCursor
                && self.profile.capabilities().supports(Capability::ResumableResponse)
                && persisted.continuation().sequence().is_some()
                && persisted.continuation().event_id().is_some()
                && !persisted.prefix().is_empty();
            if !exact_profile || !exact_resume {
                return Ok(ContinuationRestoreOutcome::Unsupported);
            }
            let mut reducer = ResponseReducer::new(
                self.profile.provider().clone(),
                self.config.protocol_limits(),
            );
            for envelope in persisted.prefix() {
                let _ = reducer.push(envelope.clone()).map_err(|_| {
                    error::invalid("persisted OpenAI continuation prefix is invalid")
                })?;
            }
            if reducer.terminal().is_some()
                || reducer.header_rejection().is_some()
                || reducer.continuation(ResumeKind::ExactCursor).as_ref()
                    != Some(persisted.continuation())
            {
                return Err(error::invalid(
                    "persisted OpenAI continuation does not match its reduced cursor",
                ));
            }
            let sequence = persisted.continuation().sequence().ok_or_else(|| {
                error::invalid("persisted OpenAI continuation omitted its exact sequence")
            })?;
            let Some(restored) = OpenAiResumeState::restore(
                persisted.prefix(),
                persisted.continuation().response_id(),
                sequence,
                self.profile.model(),
                self.config.protocol_limits(),
            )?
            else {
                return Ok(ContinuationRestoreOutcome::Unsupported);
            };
            let mut known = self
                .resumable_background
                .lock()
                .map_err(|_| error::invalid("OpenAI continuation registry is unavailable"))?;
            let mut registry = self
                .restored_continuations
                .lock()
                .map_err(|_| {
                    error::invalid("OpenAI restored continuation registry is unavailable")
                })?;
            let response_id = persisted.continuation().response_id().clone();
            if (!known.contains(&response_id) && known.len() >= 4_096)
                || (!registry.contains_key(&response_id) && registry.len() >= 4_096)
            {
                return Err(error::limit("OpenAI continuation registry capacity was exceeded"));
            }
            known.insert(response_id.clone());
            registry.insert(response_id, restored);
            Ok(ContinuationRestoreOutcome::Restored(persisted.continuation().clone()))
        })
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
