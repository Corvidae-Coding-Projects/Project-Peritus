//! Immediate header rejection followed by bounded optional body evidence.

use peritus_model_protocol::{
    EventEnvelope, ModelEvent, ModelFailure, ResponseBodyCompletion, ResponseBodyObservation,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, ModelStream, ProviderCoreError,
    ProviderCoreErrorKind,
};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

pub(super) struct GoogleRejectionStream {
    body: Box<dyn ByteStream>,
    failure: ModelFailure,
    maximum: usize,
    observed_bytes: usize,
    hasher: Sha256,
    header_emitted: bool,
    terminal: bool,
}

impl GoogleRejectionStream {
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
        EventEnvelope::new(sequence, None, None, digest, event).map_err(|_| {
            ProviderCoreError::malformed_stream(
                "google_rejection",
                "Google rejection envelope was invalid",
            )
        })
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

impl ModelStream for GoogleRejectionStream {
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
                    Err(error) => {
                        break match error.kind() {
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
