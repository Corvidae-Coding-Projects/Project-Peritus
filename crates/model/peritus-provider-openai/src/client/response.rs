//! Bounded response-body handling and normalized pre-stream failures.

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ModelFailure, OutcomeCertainty, ProviderProfile,
    ResponseBodyCompletion, ResponseBodyObservation, Retryability, TransportPhase,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, HttpHeaders, ModelStream, OwnedModelStream,
    ProviderCoreError, ProviderCoreErrorKind,
};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{error, stream::OpenAiStream};

pub(super) struct OpenAiRejectionStream {
    body: Box<dyn ByteStream>,
    failure: ModelFailure,
    maximum: usize,
    observed_bytes: usize,
    hasher: Sha256,
    header_emitted: bool,
    terminal: bool,
}

impl OpenAiRejectionStream {
    pub(super) fn new(body: Box<dyn ByteStream>, failure: ModelFailure, maximum: usize) -> Self {
        Self {
            body,
            failure,
            maximum,
            observed_bytes: 0,
            hasher: Sha256::new(),
            header_emitted: false,
            terminal: false,
        }
    }

    fn envelope(
        sequence: u64,
        digest: Sha256Digest,
        event: ModelEvent,
    ) -> Result<EventEnvelope, ProviderCoreError> {
        EventEnvelope::new(sequence, None, None, digest, event)
            .map_err(|_| error::malformed("OpenAI rejection envelope was invalid"))
    }

    fn finish(
        &mut self,
        completion: ResponseBodyCompletion,
    ) -> Result<EventEnvelope, ProviderCoreError> {
        self.terminal = true;
        let digest = Sha256Digest::new(std::mem::take(&mut self.hasher).finalize().into());
        let observed_bytes = u64::try_from(self.observed_bytes).unwrap_or(u64::MAX);
        let body = ResponseBodyObservation::new(digest, observed_bytes, completion);
        let failure = self.failure.clone().with_response_body_observation(body);
        Self::envelope(2, digest, ModelEvent::ResponseFailed(failure))
    }
}

impl ModelStream for OpenAiRejectionStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            if self.terminal {
                return Ok(None);
            }
            if !self.header_emitted {
                self.header_emitted = true;
                let digest = peritus_codec::sha256(self.failure.diagnostic().code().as_bytes());
                return Self::envelope(
                    1,
                    digest,
                    ModelEvent::ResponseRejected(self.failure.clone()),
                )
                .map(Some);
            }
            let completion = loop {
                match self.body.next(cancellation).await {
                    Ok(Some(chunk)) => {
                        let Some(next) = self.observed_bytes.checked_add(chunk.len()) else {
                            break ResponseBodyCompletion::ExceededBound;
                        };
                        if next > self.maximum {
                            break ResponseBodyCompletion::ExceededBound;
                        }
                        self.hasher.update(&chunk);
                        self.observed_bytes = next;
                    }
                    Ok(None) => break ResponseBodyCompletion::Complete,
                    Err(failure) => {
                        break match failure.kind() {
                            ProviderCoreErrorKind::Cancelled => ResponseBodyCompletion::Cancelled,
                            ProviderCoreErrorKind::LimitExceeded => {
                                ResponseBodyCompletion::ExceededBound
                            }
                            _ => ResponseBodyCompletion::TransportFailed,
                        };
                    }
                }
            };
            self.finish(completion).map(Some)
        })
    }
}

pub(super) fn connection_failure(
    profile: &ProviderProfile,
    cancellation: &CancellationToken,
) -> Result<OwnedModelStream, ProviderCoreError> {
    failure_stream(
        profile,
        cancellation,
        FailureCategory::Transport,
        TransportPhase::Connecting,
        OutcomeCertainty::DefinitelyNotAccepted,
        Retryability::SafeNewRequest,
        "openai.connect.failed",
    )
}

pub(super) fn ambiguous_failure(
    profile: &ProviderProfile,
    cancellation: &CancellationToken,
) -> Result<OwnedModelStream, ProviderCoreError> {
    failure_stream(
        profile,
        cancellation,
        FailureCategory::AmbiguousAcceptance,
        TransportPhase::SendingBody,
        OutcomeCertainty::MaybeAccepted,
        Retryability::CallerDecision,
        "openai.submission.ambiguous",
    )
}

pub(super) fn is_event_stream(headers: &HttpHeaders) -> bool {
    headers
        .first("content-type")
        .and_then(|value| value.nonsensitive_bytes())
        .and_then(|bytes| core::str::from_utf8(bytes).ok())
        .is_some_and(|value| {
            value.split(';').next().is_some_and(|media_type| {
                media_type.trim().eq_ignore_ascii_case("text/event-stream")
            })
        })
}

pub(super) fn failure_stream(
    profile: &ProviderProfile,
    cancellation: &CancellationToken,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    code: &'static str,
) -> Result<OwnedModelStream, ProviderCoreError> {
    let failure = error::failure(
        profile.provider(),
        category,
        phase,
        certainty,
        retryability,
        None,
        None,
        None,
        code,
    )?;
    let stream = OpenAiStream::failure_stream(
        profile.provider().clone(),
        ModelEvent::ResponseFailed(failure),
        peritus_codec::sha256(code.as_bytes()),
    )?;
    Ok(OwnedModelStream::new(stream, cancellation.clone()))
}
