//! Orthogonal generation, cache, persistence, and continuation options.

use crate::{
    BoundedText, EventId, ProtocolError, ProtocolErrorKind, ProviderExtension, ReasoningPolicy,
    ResponseId, StructuredOutput,
};

/// Prompt/provider cache behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CachePolicy {
    /// Do not request explicit caching.
    Disabled,
    /// Allow provider automatic/implicit caching.
    Automatic,
    /// Request an ephemeral cache breakpoint with bounded TTL seconds.
    Ephemeral {
        /// Requested cache lifetime in seconds.
        ttl_seconds: u32,
    },
    /// Reuse one opaque provider cache identity.
    Explicit(crate::CacheKey),
}

/// Provider-side storage/background policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistencePolicy {
    store: bool,
    background: bool,
}

impl PersistencePolicy {
    /// Local-first default: no provider storage and no background execution.
    pub const LOCAL_FIRST: Self = Self { store: false, background: false };

    /// Creates an explicit policy.
    ///
    /// # Errors
    ///
    /// Rejects background execution without provider storage.
    pub fn new(store: bool, background: bool) -> Result<Self, ProtocolError> {
        if background && !store {
            return Err(invalid(
                "persistence",
                "background execution requires explicit provider storage",
            ));
        }
        Ok(Self { store, background })
    }

    /// Returns whether provider storage is requested.
    #[must_use]
    pub const fn store(self) -> bool {
        self.store
    }
    /// Returns whether background execution is requested.
    #[must_use]
    pub const fn background(self) -> bool {
        self.background
    }
}

/// Provider continuation identity and optional exact cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Continuation {
    response_id: ResponseId,
    event_id: Option<EventId>,
    sequence: Option<u64>,
}

impl Continuation {
    /// Creates semantic or exact-cursor continuation.
    ///
    /// # Errors
    ///
    /// Rejects sequence zero and an event identity without a sequence.
    pub fn new(
        response_id: ResponseId,
        event_id: Option<EventId>,
        sequence: Option<u64>,
    ) -> Result<Self, ProtocolError> {
        if sequence == Some(0) || (event_id.is_some() && sequence.is_none()) {
            return Err(invalid("continuation", "continuation cursor is structurally invalid"));
        }
        Ok(Self { response_id, event_id, sequence })
    }

    /// Borrows the prior provider response identity.
    #[must_use]
    pub const fn response_id(&self) -> &ResponseId {
        &self.response_id
    }
    /// Borrows the optional provider event identity.
    #[must_use]
    pub const fn event_id(&self) -> Option<&EventId> {
        self.event_id.as_ref()
    }
    /// Returns the optional exact sequence cursor.
    #[must_use]
    pub const fn sequence(&self) -> Option<u64> {
        self.sequence
    }
    pub(crate) const fn is_exact(&self) -> bool {
        self.sequence.is_some()
    }
}

/// Portable generation ceilings and explicitly capability-gated sampling controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationConfig {
    max_output_tokens: u64,
    stop_sequences: Vec<BoundedText>,
    seed: Option<i64>,
    temperature_millionths: Option<u32>,
    top_p_millionths: Option<u32>,
}

impl GenerationConfig {
    /// Creates checked generation controls.
    ///
    /// # Errors
    ///
    /// Rejects zero output, too many stop strings, temperature above two, or top-p above one.
    pub fn new(
        max_output_tokens: u64,
        stop_sequences: Vec<BoundedText>,
        seed: Option<i64>,
        temperature_millionths: Option<u32>,
        top_p_millionths: Option<u32>,
    ) -> Result<Self, ProtocolError> {
        if max_output_tokens == 0
            || stop_sequences.len() > 64
            || temperature_millionths.is_some_and(|value| value > 2_000_000)
            || top_p_millionths.is_some_and(|value| value > 1_000_000)
        {
            return Err(invalid("generation", "generation controls exceed their portable bounds"));
        }
        Ok(Self {
            max_output_tokens,
            stop_sequences,
            seed,
            temperature_millionths,
            top_p_millionths,
        })
    }

    /// Maximum output token request.
    #[must_use]
    pub const fn max_output_tokens(&self) -> u64 {
        self.max_output_tokens
    }
    /// Borrows stop sequences.
    #[must_use]
    pub fn stop_sequences(&self) -> &[BoundedText] {
        &self.stop_sequences
    }
    /// Deterministic seed when supported.
    #[must_use]
    pub const fn seed(&self) -> Option<i64> {
        self.seed
    }
    /// Temperature in millionths.
    #[must_use]
    pub const fn temperature_millionths(&self) -> Option<u32> {
        self.temperature_millionths
    }
    /// Top-p in millionths.
    #[must_use]
    pub const fn top_p_millionths(&self) -> Option<u32> {
        self.top_p_millionths
    }

    pub(crate) const fn uses_sampling(&self) -> bool {
        self.seed.is_some()
            || self.temperature_millionths.is_some()
            || self.top_p_millionths.is_some()
    }
}

/// Remaining orthogonal request policies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestOptions {
    output: StructuredOutput,
    reasoning: ReasoningPolicy,
    generation: GenerationConfig,
    cache: CachePolicy,
    persistence: PersistencePolicy,
    continuation: Option<Continuation>,
    extensions: Vec<ProviderExtension>,
}

impl RequestOptions {
    /// Creates an options bundle; complete capability checks occur in [`crate::ModelRequest::new`].
    #[must_use]
    pub const fn new(
        output: StructuredOutput,
        reasoning: ReasoningPolicy,
        generation: GenerationConfig,
        cache: CachePolicy,
        persistence: PersistencePolicy,
        continuation: Option<Continuation>,
        extensions: Vec<ProviderExtension>,
    ) -> Self {
        Self { output, reasoning, generation, cache, persistence, continuation, extensions }
    }

    /// Output contract.
    #[must_use]
    pub const fn output(&self) -> &StructuredOutput {
        &self.output
    }
    /// Reasoning controls.
    #[must_use]
    pub const fn reasoning(&self) -> ReasoningPolicy {
        self.reasoning
    }
    /// Generation controls.
    #[must_use]
    pub const fn generation(&self) -> &GenerationConfig {
        &self.generation
    }
    /// Cache policy.
    #[must_use]
    pub const fn cache(&self) -> &CachePolicy {
        &self.cache
    }
    /// Persistence policy.
    #[must_use]
    pub const fn persistence(&self) -> PersistencePolicy {
        self.persistence
    }
    /// Continuation.
    #[must_use]
    pub const fn continuation(&self) -> Option<&Continuation> {
        self.continuation.as_ref()
    }
    /// Provider extensions.
    #[must_use]
    pub fn extensions(&self) -> &[ProviderExtension] {
        &self.extensions
    }
}

fn invalid(path: &'static str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidRequest, path, detail)
}
