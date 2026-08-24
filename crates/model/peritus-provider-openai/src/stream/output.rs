//! Heterogeneous output-item, content-part, reasoning, and tool-fragment normalization.

use peritus_model_protocol::{ItemId, ItemKind, ModelEvent, StreamFragment, ToolCallId, ToolName};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::{OpenAiStream, state};
use crate::error;

impl OpenAiStream {
    pub(super) fn output_item_added(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let output_index = u32_field(value, "output_index")?;
        let item = object_field(value, "item")?;
        let wire_id = string_field(item, "id")?;
        let normalized_id = ItemId::new(wire_id.to_owned())
            .map_err(|_| error::malformed("OpenAI output item identity is invalid"))?;
        let item_type = string_field(item, "type")?;
        let kind = match item_type {
            "message" => ItemKind::Message,
            "function_call" | "custom_tool_call" => ItemKind::ToolCall,
            "reasoning" => ItemKind::Reasoning,
            known if provider_native_item(known) => ItemKind::ProviderNative,
            _ => return Err(error::malformed("unknown correctness-critical OpenAI item type")),
        };
        let mut events = Vec::new();
        if kind != ItemKind::Message {
            events.push(ModelEvent::ItemStarted {
                item_id: normalized_id.clone(),
                index: normalized_index(output_index, 0)?,
                kind,
            });
        }
        let (call_id, call_name) = if kind == ItemKind::ToolCall {
            let call_id = ToolCallId::new(string_field(item, "call_id")?.to_owned())
                .map_err(|_| error::malformed("OpenAI tool-call identity is invalid"))?;
            let name = ToolName::new(string_field(item, "name")?.to_owned())
                .map_err(|_| error::malformed("OpenAI tool name is invalid"))?;
            events.push(ModelEvent::ToolCallStarted {
                item_id: normalized_id.clone(),
                call_id: call_id.clone(),
                name: name.clone(),
            });
            (Some(call_id), Some(name))
        } else {
            (None, None)
        };
        let inserted = self.state.insert_item(
            wire_id.to_owned(),
            state::ItemState {
                normalized_id,
                output_index,
                kind,
                call_id,
                call_name,
                arguments: Vec::new(),
                arguments_done: false,
                completed: false,
            },
        );
        if !inserted {
            return Err(error::malformed("OpenAI output item was added more than once"));
        }
        Ok(events)
    }

    pub(super) fn content_part_added(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (wire_id, output_index, content_index) = content_coordinates(value)?;
        let item = self
            .state
            .item(wire_id)
            .ok_or_else(|| error::malformed("OpenAI content part preceded its output item"))?;
        if item.kind != ItemKind::Message || item.output_index != output_index || item.completed {
            return Err(error::malformed("OpenAI content part targeted an incompatible item"));
        }
        let part = object_field(value, "part")?;
        let kind = match string_field(part, "type")? {
            "output_text" => {
                if self.structured_output {
                    ItemKind::StructuredOutput
                } else {
                    ItemKind::Message
                }
            }
            "refusal" => ItemKind::Refusal,
            _ => return Err(error::malformed("unknown OpenAI message content-part type")),
        };
        let normalized_id = normalized_part_id(wire_id, content_index)?;
        let inserted = self.state.insert_part(
            wire_id.to_owned(),
            content_index,
            state::PartState {
                normalized_id: normalized_id.clone(),
                output_index,
                content_index,
                kind,
                bytes: Vec::new(),
                value_done: false,
                completed: false,
            },
        );
        if !inserted {
            return Err(error::malformed("OpenAI content part was added more than once"));
        }
        Ok(vec![ModelEvent::ItemStarted {
            item_id: normalized_id,
            index: normalized_index(output_index, content_index)?,
            kind,
        }])
    }

    pub(super) fn content_delta(
        &mut self,
        value: &Value,
        refusal: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (wire_id, output_index, content_index) = content_coordinates(value)?;
        let delta = string_field(value, "delta")?.as_bytes();
        let limits = self.limits;
        let part = self
            .state
            .part_mut(wire_id, content_index)
            .ok_or_else(|| error::malformed("OpenAI content delta preceded content-part start"))?;
        if part.output_index != output_index
            || part.content_index != content_index
            || part.value_done
            || part.completed
            || refusal != (part.kind == ItemKind::Refusal)
        {
            return Err(error::malformed("OpenAI content delta targeted an incompatible part"));
        }
        append_bounded(&mut part.bytes, delta, limits.max_output_bytes())?;
        let fragment = StreamFragment::new(delta.to_vec(), limits)
            .map_err(|_| error::limit("OpenAI content fragment exceeds protocol limits"))?;
        let event = if refusal {
            ModelEvent::RefusalDelta { item_id: part.normalized_id.clone(), fragment }
        } else {
            ModelEvent::TextDelta { item_id: part.normalized_id.clone(), fragment }
        };
        Ok(vec![event])
    }

    pub(super) fn content_value_done(
        &mut self,
        value: &Value,
        refusal: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (wire_id, output_index, content_index) = content_coordinates(value)?;
        let field = if refusal { "refusal" } else { "text" };
        let complete = string_field(value, field)?.as_bytes();
        let part = self
            .state
            .part_mut(wire_id, content_index)
            .ok_or_else(|| error::malformed("OpenAI content done preceded content-part start"))?;
        if part.output_index != output_index
            || refusal != (part.kind == ItemKind::Refusal)
            || part.value_done
            || part.bytes != complete
        {
            return Err(error::malformed("OpenAI finalized content contradicted its deltas"));
        }
        part.value_done = true;
        Ok(Vec::new())
    }

    pub(super) fn content_part_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (wire_id, output_index, content_index) = content_coordinates(value)?;
        let part = self
            .state
            .part_mut(wire_id, content_index)
            .ok_or_else(|| error::malformed("OpenAI content-part done preceded start"))?;
        if part.output_index != output_index || !part.value_done || part.completed {
            return Err(error::malformed("OpenAI content-part terminal was inconsistent"));
        }
        part.completed = true;
        Ok(vec![ModelEvent::ItemCompleted(part.normalized_id.clone())])
    }

    pub(super) fn tool_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let wire_id = string_field(value, "item_id")?;
        let output_index = u32_field(value, "output_index")?;
        let delta = string_field(value, "delta")?.as_bytes();
        let limits = self.limits;
        let item = self
            .state
            .item_mut(wire_id)
            .ok_or_else(|| error::malformed("OpenAI tool delta preceded its item"))?;
        if item.kind != ItemKind::ToolCall
            || item.output_index != output_index
            || item.arguments_done
            || item.completed
        {
            return Err(error::malformed("OpenAI tool delta targeted an incompatible item"));
        }
        append_bounded(&mut item.arguments, delta, limits.max_tool_argument_bytes())?;
        let call_id = item
            .call_id
            .clone()
            .ok_or_else(|| error::malformed("OpenAI tool item omitted its call identity"))?;
        let fragment = StreamFragment::new(delta.to_vec(), limits)
            .map_err(|_| error::limit("OpenAI tool fragment exceeds protocol limits"))?;
        Ok(vec![ModelEvent::ToolArgumentDelta { call_id, fragment }])
    }

    pub(super) fn tool_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let wire_id = string_field(value, "item_id")?;
        let output_index = u32_field(value, "output_index")?;
        let arguments = string_field_any(value, &["arguments", "input"])?;
        let item = self
            .state
            .item_mut(wire_id)
            .ok_or_else(|| error::malformed("OpenAI tool done preceded its item"))?;
        if item.kind != ItemKind::ToolCall
            || item.output_index != output_index
            || item.arguments_done
            || item.arguments != arguments.as_bytes()
        {
            return Err(error::malformed("OpenAI finalized tool input contradicted its deltas"));
        }
        if let Some(name) = value.get("name").and_then(Value::as_str)
            && item.call_name.as_ref().is_none_or(|known| known.as_str() != name)
        {
            return Err(error::malformed("OpenAI finalized tool name changed"));
        }
        item.arguments_done = true;
        Ok(Vec::new())
    }

    pub(super) fn reasoning_delta(
        &self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let wire_id = string_field(value, "item_id")?;
        let output_index = u32_field(value, "output_index")?;
        let delta = string_field(value, "delta")?.as_bytes();
        let item = self
            .state
            .item(wire_id)
            .ok_or_else(|| error::malformed("OpenAI reasoning delta preceded its item"))?;
        if item.kind != ItemKind::Reasoning || item.output_index != output_index || item.completed {
            return Err(error::malformed("OpenAI reasoning delta targeted an incompatible item"));
        }
        let fragment = StreamFragment::new(delta.to_vec(), self.limits)
            .map_err(|_| error::limit("OpenAI reasoning fragment exceeds protocol limits"))?;
        Ok(vec![ModelEvent::ReasoningSummaryDelta {
            item_id: item.normalized_id.clone(),
            fragment,
        }])
    }

    pub(super) fn output_item_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let output_index = u32_field(value, "output_index")?;
        let wire = object_field(value, "item")?;
        let wire_id = string_field(wire, "id")?;
        let parts_complete = self.state.parts_for_item_complete(wire_id);
        let limits = self.limits;
        let item = self
            .state
            .item_mut(wire_id)
            .ok_or_else(|| error::malformed("OpenAI output-item done preceded start"))?;
        if item.output_index != output_index || item.completed {
            return Err(error::malformed("OpenAI output-item terminal was inconsistent"));
        }
        let mut events = Vec::new();
        match item.kind {
            ItemKind::Message if !parts_complete => {
                return Err(error::malformed("OpenAI message ended before all content parts"));
            }
            ItemKind::ToolCall if !item.arguments_done => {
                return Err(error::malformed("OpenAI tool item ended before finalized arguments"));
            }
            ItemKind::Reasoning => {
                if let Some(encrypted) = wire.get("encrypted_content").and_then(Value::as_str) {
                    let fragment = StreamFragment::new(encrypted.as_bytes().to_vec(), limits)
                        .map_err(|_| error::limit("OpenAI reasoning replay exceeds limits"))?;
                    events.push(ModelEvent::ReasoningReplayDelta {
                        item_id: item.normalized_id.clone(),
                        fragment,
                    });
                }
                events.push(ModelEvent::ItemCompleted(item.normalized_id.clone()));
            }
            ItemKind::ProviderNative | ItemKind::ToolCall => {
                events.push(ModelEvent::ItemCompleted(item.normalized_id.clone()));
            }
            ItemKind::Message => {}
            ItemKind::StructuredOutput | ItemKind::Refusal => {
                return Err(error::malformed("invalid OpenAI output item state"));
            }
        }
        item.completed = true;
        Ok(events)
    }
}

fn content_coordinates(value: &Value) -> Result<(&str, u32, u32), ProviderCoreError> {
    Ok((
        string_field(value, "item_id")?,
        u32_field(value, "output_index")?,
        u32_field(value, "content_index")?,
    ))
}

fn normalized_part_id(wire_id: &str, content_index: u32) -> Result<ItemId, ProviderCoreError> {
    let value = if content_index == 0 {
        wire_id.to_owned()
    } else {
        format!("{wire_id}-part-{content_index}")
    };
    ItemId::new(value)
        .map_err(|_| error::malformed("OpenAI normalized content identity is invalid"))
}

fn normalized_index(output_index: u32, content_index: u32) -> Result<u32, ProviderCoreError> {
    output_index
        .checked_mul(65_536)
        .and_then(|value| value.checked_add(content_index))
        .ok_or_else(|| error::limit("OpenAI item/content index exceeds normalized bounds"))
}

fn append_bounded(
    target: &mut Vec<u8>,
    bytes: &[u8],
    maximum: usize,
) -> Result<(), ProviderCoreError> {
    if target.len().checked_add(bytes.len()).is_none_or(|total| total > maximum) {
        return Err(error::limit("OpenAI fragmented output exceeds its aggregate bound"));
    }
    target.extend_from_slice(bytes);
    Ok(())
}

fn provider_native_item(value: &str) -> bool {
    matches!(
        value,
        "web_search_call"
            | "file_search_call"
            | "computer_call"
            | "code_interpreter_call"
            | "image_generation_call"
            | "mcp_call"
            | "shell_call"
    )
}

fn object_field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, ProviderCoreError> {
    value
        .get(name)
        .filter(|value| value.is_object())
        .ok_or_else(|| error::malformed("OpenAI event omitted a required object"))
}

fn string_field<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error::malformed("OpenAI event omitted a required string"))
}

fn string_field_any<'a>(value: &'a Value, names: &[&str]) -> Result<&'a str, ProviderCoreError> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .ok_or_else(|| error::malformed("OpenAI event omitted finalized tool input"))
}

fn u32_field(value: &Value, name: &str) -> Result<u32, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| error::malformed("OpenAI event omitted a bounded index"))
}
