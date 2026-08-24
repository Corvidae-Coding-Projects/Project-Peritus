//! Provider-neutral, versioned, bounded model protocol for Peritus.
//!
//! Constructors validate all values before they cross the provider boundary. Provider wire types,
//! credentials, transport handles, and tool authorization are deliberately outside this crate.

pub mod bounds;
mod canonical;
mod canonical_decode;
pub mod capability;
pub mod content;
pub mod error;
pub mod event;
pub mod failure;
pub mod finish;
pub mod identity;
mod json_duplicates;
pub mod message;
pub mod rate_limit;
pub mod redaction;
pub mod reducer;
pub mod request;
pub mod retry;
pub mod schema;
pub mod tool;
pub mod usage;
pub mod verified;
pub mod version;

pub use bounds::ProtocolLimits;
pub use canonical_decode::decode_request;
pub use capability::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, CapabilityState,
    ModelLimits, NegotiatedCapabilities, OutputLimitEnforcement, ProviderProfile,
    RequestedCapabilities, ResumeKind, StateMode, WireDialect, negotiate,
};
pub use content::{
    BoundedText, ContentBlock, MediaInput, MediaKind, MediaReferenceKind, MediaType,
    ProviderExtension, ReasoningReplay,
};
pub use error::{ProtocolError, ProtocolErrorKind};
pub use event::{EventEnvelope, ItemKind, ModelEvent, StreamFragment};
pub use failure::{FailureCategory, ModelFailure, OutcomeCertainty, Retryability, TransportPhase};
pub use finish::{FinishReason, TerminalOutcome};
pub use identity::{
    CacheKey, EventId, ExtensionName, IdempotencyKey, ItemId, ModelName, OutputName, ProviderName,
    RequestFingerprint, RequestId, ResponseId, ToolCallId, ToolName,
};
pub use message::{Message, Role};
pub use rate_limit::{
    CacheObservation, CacheStatus, RateLimitDimension, RateLimitObservation, RateLimitWindow,
    ResetTime,
};
pub use redaction::RedactedDiagnostic;
pub use reducer::{ReducedItem, ReducerTransition, ResponseReducer};
pub use request::{
    CachePolicy, Continuation, GenerationConfig, ModelRequest, PersistencePolicy, RequestOptions,
};
pub use retry::{
    IdempotencyGuarantee, NoRetryReason, RetryCause, RetryDecision, RetryInput, plan_retry,
};
pub use schema::{CanonicalJson, JsonBounds, JsonSchema, SchemaDialect};
pub use tool::{
    CompletedToolCall, ParallelToolPolicy, ReasoningEffort, ReasoningPolicy, StructuredOutput,
    SummaryPolicy, ToolChoice, ToolDefinition, ToolResult,
};
pub use usage::{UsageCounters, UsageObservation, UsageScope, UsageTracker};
pub use verified::{
    AuthorityObservation, DeduplicationFacts, FragmentCompletionFacts, ReducerTransitionFacts,
    RetryLegalityFacts, capability_mask_legal, deduplication_legal, fragment_completion_legal,
    next_sequence_legal, provider_observation_preserves_authority, reducer_transition_legal,
    retry_legality_complete, usage_counter_monotonic,
};
pub use version::{PROTOCOL_MAJOR, PROTOCOL_MINOR, ProtocolVersion};
