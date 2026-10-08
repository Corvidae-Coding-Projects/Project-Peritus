//! Provider-neutral normalized event grammar with bounded sensitive fragments.

use core::fmt;

use peritus_types::Sha256Digest;

use crate::{
    CacheObservation, EventId, FinishReason, ItemId, JsonBounds, ModelFailure, ModelName,
    ProtocolError, ProtocolErrorKind, ProtocolLimits, ProtocolVersion, ProviderExtension,
    RateLimitObservation, ResponseId, ToolCallId, ToolName, UsageObservation,
};

/// Sensitive stream bytes that may split UTF-8 or JSON tokens.
#[derive(Clone, Eq, PartialEq)]
pub struct StreamFragment(Vec<u8>);

impl StreamFragment {
    /// Creates one nonempty bounded fragment.
    ///
    /// # Errors
    ///
    /// Rejects empty fragments and data wider than the per-event ceiling.
    pub fn new(bytes: Vec<u8>, limits: ProtocolLimits) -> Result<Self, ProtocolError> {
        let fragment = Self(bytes);
        fragment.validate_under(limits)?;
        Ok(fragment)
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        if self.0.is_empty() || self.0.len() > limits.max_event_bytes() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEvent,
                "event.fragment",
                "stream fragment is empty or exceeds its event-byte bound",
            ));
        }
        Ok(())
    }

    /// Borrows sensitive bytes for reduction or provider projection.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Returns byte length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the fragment is empty; checked values are always nonempty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for StreamFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StreamFragment")
            .field("bytes", &self.0.len())
            .field("content", &"[redacted]")
            .finish()
    }
}

/// Kind of one independently ordered response item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemKind {
    /// Assistant text/message output.
    Message,
    /// Strict or non-strict structured JSON output.
    StructuredOutput,
    /// Application function call.
    ToolCall,
    /// Reasoning summary and opaque replay state.
    Reasoning,
    /// Refusal content.
    Refusal,
    /// Explicit provider-native item whose semantics remain adapter-owned.
    ProviderNative,
}

/// One normalized provider event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ModelEvent {
    /// Provider accepted/created a response.
    ResponseStarted {
        /// Provider response identity when supplied.
        response_id: Option<ResponseId>,
        /// Actual model identity when supplied.
        model: Option<ModelName>,
    },
    /// A response identity learned after the initial event.
    ResponseIdentity(ResponseId),
    /// One ordered output item began.
    ItemStarted {
        /// Opaque item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Semantic item kind.
        kind: ItemKind,
    },
    /// Text or structured-output bytes.
    TextDelta {
        /// Target item.
        item_id: ItemId,
        /// Ordered bytes, possibly splitting UTF-8.
        fragment: StreamFragment,
    },
    /// Visible reasoning-summary bytes.
    ReasoningSummaryDelta {
        /// Target reasoning item.
        item_id: ItemId,
        /// Ordered summary bytes.
        fragment: StreamFragment,
    },
    /// Opaque reasoning/signature bytes needed for exact replay.
    ReasoningReplayDelta {
        /// Target reasoning item.
        item_id: ItemId,
        /// Ordered sensitive replay bytes.
        fragment: StreamFragment,
    },
    /// Refusal text bytes.
    RefusalDelta {
        /// Target refusal item.
        item_id: ItemId,
        /// Ordered refusal bytes.
        fragment: StreamFragment,
    },
    /// A function call began.
    ToolCallStarted {
        /// Owning output item.
        item_id: ItemId,
        /// Opaque call identity.
        call_id: ToolCallId,
        /// Declared function name.
        name: ToolName,
    },
    /// Function arguments JSON string fragment.
    ToolArgumentDelta {
        /// Target call.
        call_id: ToolCallId,
        /// Ordered JSON bytes.
        fragment: StreamFragment,
    },
    /// One item ended and must now be validated completely.
    ItemCompleted(ItemId),
    /// Usage snapshot.
    Usage(UsageObservation),
    /// Rate-limit observation.
    RateLimit(RateLimitObservation),
    /// Provider cache observation.
    Cache(CacheObservation),
    /// Provider finish reason, not yet a terminal success by itself.
    Finish(FinishReason),
    /// Explicit bounded ancillary provider event.
    ProviderEvent(ProviderExtension),
    /// Keepalive/ping without model semantics.
    Heartbeat,
    /// Successful protocol terminal; reducer still validates all prior state.
    ResponseCompleted,
    /// Typed failure terminal.
    ResponseFailed(ModelFailure),
    /// Cancellation terminal.
    ResponseCancelled,
}

impl ModelEvent {
    fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        match self {
            Self::TextDelta { fragment, .. }
            | Self::ReasoningSummaryDelta { fragment, .. }
            | Self::ReasoningReplayDelta { fragment, .. }
            | Self::RefusalDelta { fragment, .. }
            | Self::ToolArgumentDelta { fragment, .. } => fragment.validate_under(limits),
            Self::Usage(observation) => {
                if let Some(detail) = observation.provider_detail() {
                    detail.validate_under(JsonBounds::extension(limits))?;
                }
                Ok(())
            }
            Self::Finish(FinishReason::Provider(value)) => value.validate_under(limits),
            Self::ProviderEvent(extension) => extension.validate_under(limits),
            Self::ResponseStarted { .. }
            | Self::ResponseIdentity(_)
            | Self::ItemStarted { .. }
            | Self::ToolCallStarted { .. }
            | Self::ItemCompleted(_)
            | Self::RateLimit(_)
            | Self::Cache(_)
            | Self::Finish(_)
            | Self::Heartbeat
            | Self::ResponseCompleted
            | Self::ResponseFailed(_)
            | Self::ResponseCancelled => Ok(()),
        }
    }
}

/// Event plus local/provider ordering and exact raw-event identity evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventEnvelope {
    sequence: u64,
    provider_sequence: Option<u64>,
    provider_event_id: Option<EventId>,
    provider_digest: Sha256Digest,
    event: ModelEvent,
}

impl EventEnvelope {
    /// Creates a normalized envelope.
    ///
    /// # Errors
    ///
    /// Rejects zero local/provider sequence numbers.
    pub fn new(
        sequence: u64,
        provider_sequence: Option<u64>,
        provider_event_id: Option<EventId>,
        provider_digest: Sha256Digest,
        event: ModelEvent,
    ) -> Result<Self, ProtocolError> {
        if sequence == 0 || provider_sequence == Some(0) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEvent,
                "event.sequence",
                "event sequence numbers must be nonzero",
            ));
        }
        Ok(Self { sequence, provider_sequence, provider_event_id, provider_digest, event })
    }

    /// Protocol version governing this normalized event.
    #[must_use]
    pub const fn protocol(&self) -> ProtocolVersion {
        ProtocolVersion::V1
    }

    /// Monotonic adapter-local sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Provider sequence when documented.
    #[must_use]
    pub const fn provider_sequence(&self) -> Option<u64> {
        self.provider_sequence
    }
    /// Provider event identity when supplied.
    #[must_use]
    pub const fn provider_event_id(&self) -> Option<&EventId> {
        self.provider_event_id.as_ref()
    }
    /// Digest of the exact provider event bytes.
    #[must_use]
    pub const fn provider_digest(&self) -> Sha256Digest {
        self.provider_digest
    }
    /// Borrows normalized semantics.
    #[must_use]
    pub const fn event(&self) -> &ModelEvent {
        &self.event
    }

    pub(crate) fn into_event(self) -> ModelEvent {
        self.event
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        self.event.validate_under(limits)
    }
}
