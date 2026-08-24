//! Buffered normalized events from one final Claude structured result.

use core::fmt::Write as _;
use std::collections::VecDeque;

use peritus_model_protocol::{
    CanonicalJson, EventEnvelope, FinishReason, ItemId, ItemKind, JsonBounds, ModelEvent,
    ModelFailure, ModelName, ModelRequest, ProtocolLimits, StreamFragment, ToolCallId, ToolName,
    UsageObservation, UsageScope,
};
use peritus_provider_core::{BoxFuture, CancellationToken, ModelStream, ProviderCoreError};
use peritus_types::Sha256Digest;

use super::output::RuntimeTurn;

const NORMALIZED_FRAGMENT_BYTES: usize = 4 * 1024;

pub(super) struct ClaudeRuntimeStream {
    pending: VecDeque<EventEnvelope>,
}

impl ClaudeRuntimeStream {
    pub(super) fn completed(
        request: &ModelRequest,
        turn: RuntimeTurn,
        provider_bytes: &[u8],
    ) -> Result<Self, ProviderCoreError> {
        let request_bytes = request.canonical_bytes().map_err(|_| malformed())?;
        let prefix = digest_prefix(peritus_codec::sha256(&request_bytes));
        let digest = peritus_codec::sha256(provider_bytes);
        let mut builder = Builder::new(digest);
        builder.push(ModelEvent::ResponseStarted {
            response_id: None,
            model: Some(request.model().clone()),
        })?;
        let message_id = item_id(&prefix, "message", 0)?;
        builder.push(ModelEvent::ItemStarted {
            item_id: message_id.clone(),
            index: 0,
            kind: ItemKind::Message,
        })?;
        builder.fragments(&message_id, turn.content.as_bytes())?;
        builder.push(ModelEvent::ItemCompleted(message_id))?;

        for (offset, call) in turn.tool_calls.into_iter().enumerate() {
            let index = u32::try_from(offset + 1).map_err(|_| malformed())?;
            let item_id = item_id(&prefix, "tool", index)?;
            let call_id = call_id(&prefix, index)?;
            let name = ToolName::new(call.name).map_err(|_| malformed())?;
            let arguments = serde_json::to_string(&serde_json::Value::Object(call.arguments))
                .map_err(|_| malformed())?;
            let arguments =
                CanonicalJson::parse(&arguments, JsonBounds::value(ProtocolLimits::PRODUCTION))
                    .map_err(|_| malformed())?;
            builder.push(ModelEvent::ItemStarted {
                item_id: item_id.clone(),
                index,
                kind: ItemKind::ToolCall,
            })?;
            builder.push(ModelEvent::ToolCallStarted {
                item_id: item_id.clone(),
                call_id: call_id.clone(),
                name,
            })?;
            builder.call_fragments(&call_id, arguments.canonical_bytes())?;
            builder.push(ModelEvent::ItemCompleted(item_id))?;
        }
        if has_usage(turn.usage) {
            builder.push(ModelEvent::Usage(UsageObservation::new(
                UsageScope::Final,
                turn.usage,
                None,
            )))?;
        }
        builder.push(ModelEvent::Finish(if request.tools().is_empty() {
            FinishReason::Stop
        } else if builder.has_tool_calls() {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        }))?;
        builder.push(ModelEvent::ResponseCompleted)?;
        Ok(Self { pending: builder.pending })
    }

    pub(super) fn failed(
        model: ModelName,
        failure: ModelFailure,
        digest_source: &'static [u8],
    ) -> Result<Self, ProviderCoreError> {
        Self::terminal(model, ModelEvent::ResponseFailed(failure), digest_source)
    }

    pub(super) fn cancelled(model: ModelName) -> Result<Self, ProviderCoreError> {
        Self::terminal(model, ModelEvent::ResponseCancelled, b"claude-runtime-cancelled")
    }

    fn terminal(
        model: ModelName,
        terminal: ModelEvent,
        digest_source: &'static [u8],
    ) -> Result<Self, ProviderCoreError> {
        let mut builder = Builder::new(peritus_codec::sha256(digest_source));
        builder.push(ModelEvent::ResponseStarted { response_id: None, model: Some(model) })?;
        builder.push(terminal)?;
        Ok(Self { pending: builder.pending })
    }
}

impl ModelStream for ClaudeRuntimeStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.pending.pop_front()) })
    }
}

struct Builder {
    pending: VecDeque<EventEnvelope>,
    sequence: u64,
    digest: Sha256Digest,
    tool_calls: usize,
}

impl Builder {
    const fn new(digest: Sha256Digest) -> Self {
        Self { pending: VecDeque::new(), sequence: 0, digest, tool_calls: 0 }
    }

    fn push(&mut self, event: ModelEvent) -> Result<(), ProviderCoreError> {
        self.sequence = self.sequence.checked_add(1).ok_or_else(malformed)?;
        if matches!(event, ModelEvent::ToolCallStarted { .. }) {
            self.tool_calls = self.tool_calls.checked_add(1).ok_or_else(malformed)?;
        }
        let envelope = EventEnvelope::new(self.sequence, None, None, self.digest, event)
            .map_err(|_| malformed())?;
        self.pending.push_back(envelope);
        Ok(())
    }

    fn fragments(&mut self, item_id: &ItemId, bytes: &[u8]) -> Result<(), ProviderCoreError> {
        for chunk in bytes.chunks(NORMALIZED_FRAGMENT_BYTES) {
            let fragment = StreamFragment::new(chunk.to_vec(), ProtocolLimits::PRODUCTION)
                .map_err(|_| malformed())?;
            self.push(ModelEvent::TextDelta { item_id: item_id.clone(), fragment })?;
        }
        Ok(())
    }

    fn call_fragments(
        &mut self,
        call_id: &ToolCallId,
        bytes: &[u8],
    ) -> Result<(), ProviderCoreError> {
        for chunk in bytes.chunks(NORMALIZED_FRAGMENT_BYTES) {
            let fragment = StreamFragment::new(chunk.to_vec(), ProtocolLimits::PRODUCTION)
                .map_err(|_| malformed())?;
            self.push(ModelEvent::ToolArgumentDelta { call_id: call_id.clone(), fragment })?;
        }
        Ok(())
    }

    const fn has_tool_calls(&self) -> bool {
        self.tool_calls > 0
    }
}

fn item_id(prefix: &str, kind: &str, index: u32) -> Result<ItemId, ProviderCoreError> {
    ItemId::new(format!("claude-{prefix}-{kind}-{index}")).map_err(|_| malformed())
}

fn call_id(prefix: &str, index: u32) -> Result<ToolCallId, ProviderCoreError> {
    ToolCallId::new(format!("claude-{prefix}-call-{index}")).map_err(|_| malformed())
}

fn digest_prefix(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(24);
    for byte in &digest.as_bytes()[..12] {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const fn has_usage(counters: peritus_model_protocol::UsageCounters) -> bool {
    counters.input_tokens().is_some()
        || counters.cached_input_tokens().is_some()
        || counters.cache_creation_input_tokens().is_some()
        || counters.output_tokens().is_some()
}

const fn malformed() -> ProviderCoreError {
    ProviderCoreError::malformed_stream(
        "claude_runtime_output",
        "Claude runtime result could not be normalized",
    )
}
