//! Ordered Anthropic content-block and delta normalization.

use peritus_model_protocol::{
    CanonicalJson, ItemKind, ModelEvent, ProtocolLimits, StreamFragment, ToolCallId, ToolName,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::state::{ActiveBlock, NormalizeState, Phase};
use super::value::{invalid, item_id, provider_event, replay_fragment, required_str, required_u32};

pub(super) fn start(
    state: &mut NormalizeState,
    value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    let index = required_u32(value, "/index")?;
    if !matches!(state.phase, Phase::Content)
        || index != state.next_block
        || !state.blocks.is_empty()
        || state.blocks.contains_key(&index)
    {
        return Err(invalid("Anthropic content block start is out of order"));
    }
    let response_id = state
        .response_id
        .as_ref()
        .ok_or_else(|| invalid("Anthropic content block preceded message identity"))?;
    let item = item_id(response_id, index)?;
    let block_type = required_str(value, "/content_block/type")?;
    let active = match block_type {
        "text" => {
            state.emit(
                ModelEvent::ItemStarted { item_id: item.clone(), index, kind: ItemKind::Message },
                digest,
                event_id,
            )?;
            if let Some(text) = value.pointer("/content_block/text").and_then(Value::as_str)
                && !text.is_empty()
            {
                state.emit(text_delta(item.clone(), text)?, digest, event_id)?;
            }
            ActiveBlock::Text { item_id: item }
        }
        "tool_use" => {
            let call_id = ToolCallId::new(required_str(value, "/content_block/id")?.to_owned())
                .map_err(|_| invalid("Anthropic tool-call identity is invalid"))?;
            let name = ToolName::new(required_str(value, "/content_block/name")?.to_owned())
                .map_err(|_| invalid("Anthropic tool name is invalid"))?;
            let input = value.pointer("/content_block/input").and_then(Value::as_object);
            if input.is_none_or(|input| !input.is_empty()) {
                return Err(invalid("Anthropic streamed tool input must begin as an empty object"));
            }
            state.emit(
                ModelEvent::ItemStarted { item_id: item.clone(), index, kind: ItemKind::ToolCall },
                digest,
                event_id,
            )?;
            state.emit(
                ModelEvent::ToolCallStarted {
                    item_id: item.clone(),
                    call_id: call_id.clone(),
                    name,
                },
                digest,
                event_id,
            )?;
            ActiveBlock::Tool { item_id: item, call_id, arguments: Vec::new() }
        }
        "thinking" => {
            state.emit(
                ModelEvent::ItemStarted { item_id: item.clone(), index, kind: ItemKind::Reasoning },
                digest,
                event_id,
            )?;
            if let Some(thinking) = value.pointer("/content_block/thinking").and_then(Value::as_str)
                && !thinking.is_empty()
            {
                state.emit(reasoning_delta(item.clone(), thinking)?, digest, event_id)?;
            }
            ActiveBlock::Thinking { item_id: item, signature: false }
        }
        "redacted_thinking" => {
            let data = required_str(value, "/content_block/data")?;
            state.emit(
                ModelEvent::ItemStarted { item_id: item.clone(), index, kind: ItemKind::Reasoning },
                digest,
                event_id,
            )?;
            let bytes = replay_fragment("redacted_thinking", "data", data)?;
            state.emit(replay_delta(item.clone(), bytes)?, digest, event_id)?;
            ActiveBlock::Redacted { item_id: item }
        }
        _ => return Err(invalid("Anthropic emitted an unknown correctness-critical block type")),
    };
    state.blocks.insert(index, active);
    state.next_block = state.next_block.checked_add(1).ok_or_else(|| {
        ProviderCoreError::limit_exceeded("anthropic_stream", "block index overflowed")
    })?;
    Ok(())
}

pub(super) fn delta(
    state: &mut NormalizeState,
    value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    let index = required_u32(value, "/index")?;
    if !matches!(state.phase, Phase::Content) {
        return Err(invalid("Anthropic content delta is outside the content phase"));
    }
    let delta_type = required_str(value, "/delta/type")?;
    let mut block = state
        .blocks
        .remove(&index)
        .ok_or_else(|| invalid("Anthropic content delta targets no open block"))?;
    let event = match (&mut block, delta_type) {
        (ActiveBlock::Text { item_id }, "text_delta") => {
            text_delta(item_id.clone(), required_str(value, "/delta/text")?)?
        }
        (ActiveBlock::Text { .. }, "citations_delta") => {
            provider_event("anthropic.citation", value)?
        }
        (ActiveBlock::Tool { call_id, arguments, .. }, "input_json_delta") => {
            let partial = required_str(value, "/delta/partial_json")?.as_bytes();
            if partial.is_empty() {
                return Err(invalid("Anthropic tool argument fragment is empty"));
            }
            let total = arguments
                .len()
                .checked_add(partial.len())
                .ok_or_else(|| invalid("Anthropic tool argument length overflowed"))?;
            if total > ProtocolLimits::PRODUCTION.max_tool_argument_bytes() {
                return Err(ProviderCoreError::limit_exceeded(
                    "anthropic_stream",
                    "Anthropic tool arguments exceed their byte bound",
                ));
            }
            arguments.extend_from_slice(partial);
            ModelEvent::ToolArgumentDelta {
                call_id: call_id.clone(),
                fragment: StreamFragment::new(partial.to_vec(), ProtocolLimits::PRODUCTION)
                    .map_err(|_| invalid("Anthropic tool argument fragment is invalid"))?,
            }
        }
        (ActiveBlock::Thinking { item_id, .. }, "thinking_delta") => {
            reasoning_delta(item_id.clone(), required_str(value, "/delta/thinking")?)?
        }
        (ActiveBlock::Thinking { item_id, signature }, "signature_delta") => {
            if *signature {
                return Err(invalid("Anthropic thinking block emitted multiple signatures"));
            }
            *signature = true;
            let bytes =
                replay_fragment("thinking", "signature", required_str(value, "/delta/signature")?)?;
            replay_delta(item_id.clone(), bytes)?
        }
        _ => return Err(invalid("Anthropic content delta contradicts its open block")),
    };
    state.emit(event, digest, event_id)?;
    state.blocks.insert(index, block);
    Ok(())
}

pub(super) fn stop(
    state: &mut NormalizeState,
    value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    let index = required_u32(value, "/index")?;
    if !matches!(state.phase, Phase::Content) {
        return Err(invalid("Anthropic content stop is outside the content phase"));
    }
    let block = state
        .blocks
        .remove(&index)
        .ok_or_else(|| invalid("Anthropic content stop targets no open block"))?;
    let item = match block {
        ActiveBlock::Text { item_id }
        | ActiveBlock::Redacted { item_id }
        | ActiveBlock::Thinking { item_id, signature: true } => item_id,
        ActiveBlock::Thinking { .. } => {
            return Err(invalid("Anthropic thinking block closed without a replay signature"));
        }
        ActiveBlock::Tool { item_id, call_id, arguments } => {
            let arguments = if arguments.is_empty() { b"{}".to_vec() } else { arguments };
            let text = core::str::from_utf8(&arguments)
                .map_err(|_| invalid("Anthropic tool arguments are not valid UTF-8"))?;
            let parsed = CanonicalJson::parse(
                text,
                peritus_model_protocol::JsonBounds::value(ProtocolLimits::PRODUCTION),
            )
            .map_err(|_| invalid("Anthropic tool arguments are not complete bounded JSON"))?;
            if !parsed.is_object() {
                return Err(invalid("Anthropic tool arguments are not a JSON object"));
            }
            if text == "{}" {
                state.emit(
                    ModelEvent::ToolArgumentDelta {
                        call_id,
                        fragment: StreamFragment::new(b"{}".to_vec(), ProtocolLimits::PRODUCTION)
                            .map_err(|_| {
                            invalid("empty Anthropic tool arguments are invalid")
                        })?,
                    },
                    digest,
                    event_id,
                )?;
            }
            item_id
        }
    };
    state.emit(ModelEvent::ItemCompleted(item), digest, event_id)
}

fn text_delta(
    item_id: peritus_model_protocol::ItemId,
    text: &str,
) -> Result<ModelEvent, ProviderCoreError> {
    Ok(ModelEvent::TextDelta { item_id, fragment: fragment(text.as_bytes())? })
}

fn reasoning_delta(
    item_id: peritus_model_protocol::ItemId,
    text: &str,
) -> Result<ModelEvent, ProviderCoreError> {
    Ok(ModelEvent::ReasoningSummaryDelta { item_id, fragment: fragment(text.as_bytes())? })
}

fn replay_delta(
    item_id: peritus_model_protocol::ItemId,
    bytes: Vec<u8>,
) -> Result<ModelEvent, ProviderCoreError> {
    Ok(ModelEvent::ReasoningReplayDelta {
        item_id,
        fragment: StreamFragment::new(bytes, ProtocolLimits::PRODUCTION)
            .map_err(|_| invalid("Anthropic reasoning replay fragment is invalid"))?,
    })
}

fn fragment(bytes: &[u8]) -> Result<StreamFragment, ProviderCoreError> {
    StreamFragment::new(bytes.to_vec(), ProtocolLimits::PRODUCTION)
        .map_err(|_| invalid("Anthropic content fragment is empty or exceeds bounds"))
}
