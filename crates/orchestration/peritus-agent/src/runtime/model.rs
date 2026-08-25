//! Pull-based C5 model execution with an explicit durable-acknowledgement boundary.

use core::fmt;

use peritus_model_protocol::{
    EventEnvelope, ProtocolError, ProtocolLimits, ReducedItem, ReducerTransition,
    RequestFingerprint, ResponseId, ResponseReducer, TerminalOutcome, UsageCounters,
    encode_event_envelope,
};
use peritus_provider_core::{
    CancellationToken, ModelProvider, OwnedModelStream, ProviderCoreError,
};

/// Result of one bounded model-session drive step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelAdvance {
    /// One normalized envelope is retained pending durable acknowledgement.
    EnvelopePending {
        /// Adapter-local monotonic event sequence.
        sequence: u64,
        /// Canonical encoded size that must fit the D0 event payload bound.
        encoded_bytes: usize,
    },
    /// The stream was already terminal and has no further envelopes.
    Closed,
}

/// Redaction-safe model-session failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum ModelDriveError {
    /// A caller tried to pull another event before committing the pending event.
    PendingEnvelope,
    /// A caller tried to accept an event when none was pending.
    NoPendingEnvelope,
    /// The provider transport or owned stream failed.
    Provider(ProviderCoreError),
    /// The normalized request, event codec, or response reduction failed.
    Protocol(ProtocolError),
    /// A persisted prefix was empty, terminal, or could not represent resumable in-progress work.
    InvalidContinuation,
}

impl fmt::Display for ModelDriveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PendingEnvelope => formatter
                .write_str("a normalized provider envelope is awaiting durable acknowledgement"),
            Self::NoPendingEnvelope => {
                formatter.write_str("no normalized provider envelope is pending")
            }
            Self::Provider(error) => fmt::Display::fmt(error, formatter),
            Self::Protocol(error) => fmt::Display::fmt(error, formatter),
            Self::InvalidContinuation => {
                formatter.write_str("persisted provider prefix is not resumable")
            }
        }
    }
}

impl std::error::Error for ModelDriveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::Protocol(error) => Some(error),
            Self::PendingEnvelope | Self::NoPendingEnvelope | Self::InvalidContinuation => None,
        }
    }
}

impl From<ProviderCoreError> for ModelDriveError {
    fn from(value: ProviderCoreError) -> Self {
        Self::Provider(value)
    }
}

impl From<ProtocolError> for ModelDriveError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

/// One owned provider response and its C5 normalized response reducer.
///
/// `pull_one` retains an envelope without applying it. The caller must first commit that exact
/// envelope through the D0 journal adapter and then call `accept_durable_pending`. Until that
/// acknowledgement occurs, another pull is rejected. This makes the stream cursor and logical
/// reducer unable to advance ahead of durable state.
pub struct ModelSession {
    fingerprint: RequestFingerprint,
    limits: ProtocolLimits,
    stream: OwnedModelStream,
    reducer: ResponseReducer,
    pending: Option<EventEnvelope>,
    closed: bool,
}

impl ModelSession {
    /// Starts one request whose attempt intent was already committed by the caller.
    ///
    /// # Errors
    ///
    /// Returns a protocol failure when request fingerprinting fails, or a provider-core failure
    /// when the configured provider rejects or cannot start the request.
    pub async fn start(
        provider: &dyn ModelProvider,
        request: peritus_model_protocol::ModelRequest,
        limits: ProtocolLimits,
        cancellation: CancellationToken,
    ) -> Result<Self, ModelDriveError> {
        let fingerprint = request.fingerprint()?;
        let reducer = ResponseReducer::new(request.provider().clone(), limits);
        let stream = provider.start(request, cancellation).await?;
        Ok(Self { fingerprint, limits, stream, reducer, pending: None, closed: false })
    }

    /// Rebuilds C5 reduction from durable canonical envelopes, then starts an exact continuation.
    ///
    /// The caller must first obtain an explicit restored outcome from the provider adapter and
    /// durably record the resumed attempt intent. Prefix envelopes are reduced before the provider
    /// is contacted, so corrupt local history cannot trigger a request.
    ///
    /// # Errors
    ///
    /// Rejects an empty/terminal/malformed prefix, request fingerprint failure, or provider start
    /// failure.
    pub async fn resume(
        provider: &dyn ModelProvider,
        request: peritus_model_protocol::ModelRequest,
        limits: ProtocolLimits,
        cancellation: CancellationToken,
        prefix: &[EventEnvelope],
    ) -> Result<Self, ModelDriveError> {
        if prefix.is_empty() {
            return Err(ModelDriveError::InvalidContinuation);
        }
        let fingerprint = request.fingerprint()?;
        let mut reducer = ResponseReducer::new(request.provider().clone(), limits);
        for envelope in prefix {
            let _ = reducer.push(envelope.clone())?;
        }
        if reducer.terminal().is_some() {
            return Err(ModelDriveError::InvalidContinuation);
        }
        let stream = provider.start(request, cancellation).await?;
        Ok(Self { fingerprint, limits, stream, reducer, pending: None, closed: false })
    }

    /// Returns the exact semantic request fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> RequestFingerprint {
        self.fingerprint
    }

    /// Pulls at most one normalized event and retains it for durable recording.
    ///
    /// # Errors
    ///
    /// Rejects a second pull while an envelope is pending and propagates redaction-safe provider
    /// or canonical-encoding failures.
    pub async fn pull_one(&mut self) -> Result<ModelAdvance, ModelDriveError> {
        if self.pending.is_some() {
            return Err(ModelDriveError::PendingEnvelope);
        }
        if self.closed || self.reducer.terminal().is_some() {
            self.closed = true;
            return Ok(ModelAdvance::Closed);
        }
        if let Some(envelope) = self.stream.pull().await? {
            let encoded_bytes = encode_event_envelope(&envelope, self.limits)?.len();
            let sequence = envelope.sequence();
            self.pending = Some(envelope);
            Ok(ModelAdvance::EnvelopePending { sequence, encoded_bytes })
        } else {
            let _ = self.reducer.finish_eof()?;
            self.closed = true;
            Ok(ModelAdvance::Closed)
        }
    }

    /// Borrows the exact envelope that must be included in the next durable D0 event.
    #[must_use]
    pub const fn pending(&self) -> Option<&EventEnvelope> {
        self.pending.as_ref()
    }

    /// Canonically encodes the pending envelope for a durable event payload.
    ///
    /// # Errors
    ///
    /// Rejects a missing pending envelope or a value outside the configured C5 bounds.
    pub fn encode_pending(&self) -> Result<Vec<u8>, ModelDriveError> {
        let envelope = self.pending.as_ref().ok_or(ModelDriveError::NoPendingEnvelope)?;
        encode_event_envelope(envelope, self.limits).map_err(Into::into)
    }

    /// Validates the pending envelope against a cloned reducer without advancing live state.
    ///
    /// This lets the durable driver classify exact duplicates and malformed envelopes before it
    /// records an observation. The live reducer remains unchanged until the journal commit is
    /// acknowledged through [`Self::accept_durable_pending`].
    ///
    /// # Errors
    ///
    /// Rejects a missing envelope or any C5 ordering, identity, assembly, bound, or terminal
    /// violation.
    pub fn preview_pending(&self) -> Result<ReducerTransition, ModelDriveError> {
        let envelope = self.pending.as_ref().ok_or(ModelDriveError::NoPendingEnvelope)?;
        let mut preview = self.reducer.clone();
        preview.push(envelope.clone()).map_err(Into::into)
    }

    /// Applies the pending envelope after its D0 event has durably committed.
    ///
    /// # Errors
    ///
    /// Rejects a missing envelope or any C5 ordering, identity, assembly, bound, or terminal
    /// violation. The envelope is consumed exactly once.
    pub fn accept_durable_pending(&mut self) -> Result<ReducerTransition, ModelDriveError> {
        let envelope = self.pending.take().ok_or(ModelDriveError::NoPendingEnvelope)?;
        let transition = self.reducer.push(envelope)?;
        if self.reducer.terminal().is_some() {
            self.closed = true;
        }
        Ok(transition)
    }

    /// Cancels local connection, read, and backoff work idempotently.
    pub fn cancel(&self) {
        self.stream.cancel();
    }

    /// Returns whether no more envelopes may be pulled.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Borrows the normalized terminal outcome when established.
    #[must_use]
    pub const fn terminal(&self) -> Option<&TerminalOutcome> {
        self.reducer.terminal()
    }

    /// Borrows complete reduced response items in stable provider output order.
    #[must_use]
    pub fn completed_items(&self) -> &[ReducedItem] {
        self.reducer.completed_items()
    }

    /// Borrows the provider response identity when observed.
    #[must_use]
    pub const fn response_id(&self) -> Option<&ResponseId> {
        self.reducer.response_id()
    }

    /// Returns cumulative high-water usage for B1 settlement.
    #[must_use]
    pub const fn usage_high_water(&self) -> UsageCounters {
        self.reducer.usage_high_water()
    }
}

impl fmt::Debug for ModelSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSession")
            .field("fingerprint", &self.fingerprint)
            .field("pending_sequence", &self.pending.as_ref().map(EventEnvelope::sequence))
            .field("closed", &self.closed)
            .field("reducer", &self.reducer)
            .finish_non_exhaustive()
    }
}
