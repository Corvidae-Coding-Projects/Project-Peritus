//! Owned normalized model-event streams.

use core::{
    fmt,
    future::Future as _,
    task::{Context, Poll, Waker},
};

use peritus_model_protocol::{EventEnvelope, ModelEvent};

use crate::{BoxFuture, CancellationToken, ProviderCoreError, ProviderCoreErrorKind};

/// Pull-based provider stream that emits only normalized, bounded protocol events.
pub trait ModelStream: Send {
    /// Pulls the next event.
    ///
    /// A clean `None` is legal only after a terminal event. Provider adapters must translate
    /// provider failures and cancellation into the normalized grammar once events are observable.
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>>;
}

/// A model stream that owns cancellation and enforces an explicit terminal before EOF.
pub struct OwnedModelStream {
    inner: Box<dyn ModelStream>,
    cancellation: CancellationToken,
    terminal_observed: bool,
}

impl OwnedModelStream {
    /// Owns a provider stream and the cancellation token governing all of its work.
    #[must_use]
    pub fn new<S>(inner: S, cancellation: CancellationToken) -> Self
    where
        S: ModelStream + 'static,
    {
        Self { inner: Box::new(inner), cancellation, terminal_observed: false }
    }

    /// Returns a cancellation handle for an external caller or deadline owner.
    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Cancels pending connection, read, or backoff work idempotently.
    pub fn cancel(&self) {
        let _ = self.cancellation.cancel();
    }

    /// Returns whether a normalized terminal event has been observed.
    #[must_use]
    pub const fn terminal_observed(&self) -> bool {
        self.terminal_observed
    }

    /// Pulls the next event and rejects transport EOF before a normalized terminal.
    ///
    /// # Errors
    ///
    /// Returns the underlying redaction-safe provider-core failure, or `MalformedStream` when the
    /// provider stream ends without an explicit completion, failure, or cancellation event.
    pub fn pull(&mut self) -> BoxFuture<'_, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            if self.terminal_observed {
                return Ok(None);
            }
            match self.inner.next(&self.cancellation).await {
                Ok(Some(mut envelope)) => {
                    if matches!(envelope.event(), ModelEvent::ResponseCompleted) {
                        loop {
                            match self.poll_ready_after_completion() {
                                Poll::Pending | Poll::Ready(Ok(None)) => break,
                                Poll::Ready(Ok(Some(candidate))) => {
                                    if is_unsuccessful_terminal(candidate.event()) {
                                        envelope = terminal_at_sequence(
                                            envelope.sequence(),
                                            candidate,
                                        )?;
                                        break;
                                    }
                                }
                                Poll::Ready(Err(error)) => {
                                    self.terminal_observed = true;
                                    let _ = self.cancellation.cancel();
                                    return Err(error);
                                }
                            }
                        }
                    }
                    if is_terminal(envelope.event()) {
                        self.terminal_observed = true;
                    }
                    Ok(Some(envelope))
                }
                Ok(None) => {
                    self.terminal_observed = true;
                    let _ = self.cancellation.cancel();
                    Err(ProviderCoreError::new(
                        ProviderCoreErrorKind::MalformedStream,
                        "model_stream",
                        "provider stream ended before a normalized terminal event",
                    ))
                }
                Err(error) => {
                    self.terminal_observed = true;
                    let _ = self.cancellation.cancel();
                    Err(error)
                }
            }
        })
    }

    fn poll_ready_after_completion(
        &mut self,
    ) -> Poll<Result<Option<EventEnvelope>, ProviderCoreError>> {
        let mut future = self.inner.next(&self.cancellation);
        let mut context = Context::from_waker(Waker::noop());
        future.as_mut().poll(&mut context)
    }
}

impl fmt::Debug for OwnedModelStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnedModelStream")
            .field("terminal_observed", &self.terminal_observed)
            .field("cancelled", &self.cancellation.is_cancelled())
            .field("inner", &"[private provider stream]")
            .finish()
    }
}

impl Drop for OwnedModelStream {
    fn drop(&mut self) {
        if !self.terminal_observed {
            let _ = self.cancellation.cancel();
        }
    }
}

const fn is_terminal(event: &ModelEvent) -> bool {
    matches!(
        event,
        ModelEvent::ResponseCompleted
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled
    )
}

const fn is_unsuccessful_terminal(event: &ModelEvent) -> bool {
    matches!(event, ModelEvent::ResponseFailed(_) | ModelEvent::ResponseCancelled)
}

fn terminal_at_sequence(
    sequence: u64,
    envelope: EventEnvelope,
) -> Result<EventEnvelope, ProviderCoreError> {
    EventEnvelope::new(
        sequence,
        envelope.provider_sequence(),
        envelope.provider_event_id().cloned(),
        envelope.provider_digest(),
        envelope.event().clone(),
    )
    .map_err(|_| {
        ProviderCoreError::new(
            ProviderCoreErrorKind::MalformedStream,
            "model_stream",
            "provider stream produced an invalid terminal replacement",
        )
    })
}
