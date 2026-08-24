//! Ordered bounded event reduction with fail-closed terminal classification.

mod events;
mod stream;
mod terminal;

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use peritus_types::Sha256Digest;

use crate::{
    BoundedText, CacheObservation, CanonicalJson, CompletedToolCall, EventId, FinishReason, ItemId,
    ItemKind, ProtocolLimits, ProviderExtension, ProviderName, RateLimitObservation, ResponseId,
    TerminalOutcome, ToolCallId, UsageTracker,
};

/// Fully validated completed output item.
#[derive(Clone, Eq, PartialEq)]
pub enum ReducedItem {
    /// Complete assistant text.
    Text {
        /// Item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Complete text.
        text: BoundedText,
    },
    /// Complete structured JSON.
    Structured {
        /// Item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Complete structured JSON.
        value: CanonicalJson,
    },
    /// Complete executable-shape function call; authority is still external to C5.
    ToolCall {
        /// Item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Complete function call.
        call: CompletedToolCall,
    },
    /// Complete reasoning summary plus opaque replay bytes.
    Reasoning {
        /// Item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Optional visible summary.
        summary: Option<BoundedText>,
        /// Sensitive replay state.
        replay: Vec<u8>,
    },
    /// Complete refusal text.
    Refusal {
        /// Item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Complete refusal text.
        text: BoundedText,
    },
    /// Bounded provider-native item retained without portable interpretation.
    ProviderNative {
        /// Item identity.
        item_id: ItemId,
        /// Provider output index.
        index: u32,
        /// Sensitive provider bytes.
        bytes: Vec<u8>,
    },
}

impl fmt::Debug for ReducedItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut item = formatter.debug_struct(match self {
            Self::Text { .. } => "ReducedText",
            Self::Structured { .. } => "ReducedStructuredOutput",
            Self::ToolCall { .. } => "ReducedToolCall",
            Self::Reasoning { .. } => "ReducedReasoning",
            Self::Refusal { .. } => "ReducedRefusal",
            Self::ProviderNative { .. } => "ReducedProviderNative",
        });
        match self {
            Self::Text { item_id, index, text } | Self::Refusal { item_id, index, text } => item
                .field("item_id", item_id)
                .field("index", index)
                .field("text_bytes", &text.len()),
            Self::Structured { item_id, index, value } => {
                item.field("item_id", item_id).field("index", index).field("value", value)
            }
            Self::ToolCall { item_id, index, call } => {
                item.field("item_id", item_id).field("index", index).field("call", call)
            }
            Self::Reasoning { item_id, index, summary, replay } => item
                .field("item_id", item_id)
                .field("index", index)
                .field("summary_bytes", &summary.as_ref().map(BoundedText::len))
                .field("replay_bytes", &replay.len()),
            Self::ProviderNative { item_id, index, bytes } => {
                item.field("item_id", item_id).field("index", index).field("bytes", &bytes.len())
            }
        }
        .finish_non_exhaustive()
    }
}

impl ReducedItem {
    /// Returns the provider output index used for deterministic ordering.
    #[must_use]
    pub const fn index(&self) -> u32 {
        match self {
            Self::Text { index, .. }
            | Self::Structured { index, .. }
            | Self::ToolCall { index, .. }
            | Self::Reasoning { index, .. }
            | Self::Refusal { index, .. }
            | Self::ProviderNative { index, .. } => *index,
        }
    }
}

/// Result of applying one envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReducerTransition {
    /// New event changed reducer state.
    Applied,
    /// Exact provider-event duplicate with an identical digest was ignored.
    DuplicateIgnored,
    /// One final terminal outcome was established.
    Terminal(TerminalOutcome),
}

#[derive(Clone, Debug)]
struct ToolAssembly {
    id: ToolCallId,
    name: crate::ToolName,
    arguments: Vec<u8>,
}

#[derive(Clone, Debug)]
struct ItemAssembly {
    index: u32,
    kind: ItemKind,
    content: Vec<u8>,
    replay: Vec<u8>,
    call: Option<ToolAssembly>,
    complete: bool,
}

#[derive(Clone, Debug)]
struct SeenEvent {
    digest: Sha256Digest,
    local_sequence: u64,
    provider_sequence: Option<u64>,
}

/// Stateful reducer for exactly one provider response.
#[derive(Clone)]
pub struct ResponseReducer {
    provider: ProviderName,
    limits: ProtocolLimits,
    last_sequence: u64,
    last_provider_sequence: Option<u64>,
    event_count: usize,
    output_bytes: usize,
    started: bool,
    response_id: Option<ResponseId>,
    seen: BTreeMap<EventId, SeenEvent>,
    indexes: BTreeSet<u32>,
    items: BTreeMap<ItemId, ItemAssembly>,
    calls: BTreeMap<ToolCallId, ItemId>,
    completed: Vec<ReducedItem>,
    usage: UsageTracker,
    rate_limits: Vec<RateLimitObservation>,
    cache: Vec<CacheObservation>,
    extensions: Vec<ProviderExtension>,
    finish: Option<FinishReason>,
    terminal: Option<TerminalOutcome>,
}

impl fmt::Debug for ResponseReducer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponseReducer")
            .field("provider", &self.provider)
            .field("last_sequence", &self.last_sequence)
            .field("last_provider_sequence", &self.last_provider_sequence)
            .field("event_count", &self.event_count)
            .field("output_bytes", &self.output_bytes)
            .field("started", &self.started)
            .field("response_id", &self.response_id)
            .field("seen_events", &self.seen.len())
            .field("open_items", &self.items.len())
            .field("completed_items", &self.completed.len())
            .field("tool_calls", &self.calls.len())
            .field("terminal", &self.terminal)
            .finish_non_exhaustive()
    }
}
