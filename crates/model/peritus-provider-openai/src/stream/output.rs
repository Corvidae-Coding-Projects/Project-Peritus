//! Heterogeneous output-item, content-part, reasoning, and tool-fragment normalization.

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, ItemId, ItemKind, JsonBounds, ModelEvent, ProtocolLimits,
    ProviderExtension, StreamFragment, ToolCallId, ToolName,
};
use peritus_provider_core::{ProviderCoreError, healing::StructuredOutputBuffer};
use serde_json::Value;

use super::{OpenAiStream, identity, state};
use crate::error;

impl OpenAiStream {
    pub(super) fn output_item_added(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let output_index = u32_field(value, "output_index")?;
        let item = object_field(value, "item")?;
        let wire_id = string_field(item, "id")?;
        let item_type = string_field(item, "type")?;
        let kind = match item_type {
            "message" => ItemKind::Message,
            "function_call" | "custom_tool_call" => ItemKind::ToolCall,
            "reasoning" => ItemKind::Reasoning,
            known if provider_native_item(known) => ItemKind::ProviderNative,
            _ => return Err(error::malformed("unknown correctness-critical OpenAI item type")),
        };
        let preferred = identity::item_id(wire_id, "")?;
        let normalized_id = if kind == ItemKind::Message {
            preferred
        } else {
            self.claim_item_id(wire_id, "", preferred)?
        };
        let mut events = Vec::new();
        if kind == ItemKind::Message {
            events = Self::ancillary(value)?;
        } else {
            let index = self
                .state
                .normalized_index(state::NormalizedCoordinate::Output(output_index))
                .ok_or_else(|| error::limit("OpenAI normalized item indexes were exhausted"))?;
            let legacy_index = output_index.checked_mul(65_536);
            if normalized_id.expose_for_wire() != wire_id || legacy_index != Some(index) {
                events = Self::ancillary(value)?;
            }
            events.push(ModelEvent::ItemStarted {
                item_id: normalized_id.clone(),
                index,
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
                arguments: peritus_provider_core::healing::ToolArgumentBuffer::new(),
                final_arguments: None,
                argument_progress_revision: 0,
                arguments_done: false,
                completed: false,
                parts_started: 0,
                parts_completed: 0,
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
        let suffix = if content_index == 0 {
            String::new()
        } else {
            format!("-part-{content_index}")
        };
        let preferred = identity::item_id(wire_id, &suffix)?;
        let normalized_id = self.claim_item_id(wire_id, &suffix, preferred)?;
        let index = self
            .state
            .normalized_index(state::NormalizedCoordinate::Content {
                output: output_index,
                content: content_index,
            })
            .ok_or_else(|| error::limit("OpenAI normalized content indexes were exhausted"))?;
        let inserted = self.state.insert_part(
            wire_id.to_owned(),
            content_index,
            state::PartState {
                normalized_id: normalized_id.clone(),
                output_index,
                content_index,
                kind,
                observed: state::ObservedValue::new(),
                validated: state::ObservedValue::new(),
                structured: (kind == ItemKind::StructuredOutput)
                    .then(StructuredOutputBuffer::new),
                progress_revision: 0,
                value_done: false,
                completed: false,
            },
        );
        if !inserted {
            return Err(error::malformed("OpenAI content part was added more than once"));
        }
        let legacy_index = output_index
            .checked_mul(65_536)
            .and_then(|base| base.checked_add(content_index));
        let retained_legacy_id = normalized_id
            .expose_for_wire()
            .strip_prefix(wire_id)
            .is_some_and(|tail| tail == suffix.as_str());
        let mut events = if !retained_legacy_id || legacy_index != Some(index)
        {
            Self::ancillary(value)?
        } else {
            Vec::new()
        };
        events.push(ModelEvent::ItemStarted { item_id: normalized_id, index, kind });
        Ok(events)
    }

    pub(super) fn content_delta(
        &mut self,
        value: &Value,
        refusal: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (wire_id, output_index, content_index) = content_coordinates(value)?;
        let delta = string_field_allow_empty(value, "delta")?.as_bytes();
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
        part.observed.observe(delta, limits.max_output_bytes())?;
        if part.kind == ItemKind::StructuredOutput {
            part.structured
                .as_mut()
                .ok_or_else(|| error::malformed("OpenAI structured buffer was unavailable"))?
                .append(delta, limits)?;
            if delta.is_empty() {
                return Self::ancillary(value);
            }
            part.progress_revision = part.progress_revision.checked_add(1).ok_or_else(|| {
                error::limit("OpenAI structured progress revision overflowed")
            })?;
            let fragment = StreamFragment::new(delta.to_vec(), limits)
                .map_err(|_| error::limit("OpenAI structured fragment exceeds protocol limits"))?;
            return Ok(vec![ModelEvent::StructuredOutputProgress {
                item_id: part.normalized_id.clone(),
                revision: part.progress_revision,
                fragment,
            }]);
        }
        if delta.is_empty() {
            return Self::ancillary(value);
        }
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
        let complete = string_field_allow_empty(value, field)?.as_bytes();
        let limits = self.limits;
        let (completion, marker) = {
            let part = self
                .state
                .part_mut(wire_id, content_index)
                .ok_or_else(|| error::malformed("OpenAI content done preceded content-part start"))?;
            if part.output_index != output_index
                || refusal != (part.kind == ItemKind::Refusal)
                || part.value_done
                || !part.observed.matches_bytes(complete)
            {
                return Err(error::malformed("OpenAI finalized content contradicted its deltas"));
            }
            part.value_done = true;
            if part.kind == ItemKind::StructuredOutput {
                let buffer = part.structured.take().ok_or_else(|| {
                    error::malformed("OpenAI structured completion lost its buffer")
                })?;
                (
                    Some(buffer.into_completion(part.normalized_id.clone(), limits)?),
                    None,
                )
            } else {
                (
                    None,
                    Some(content_done_marker(
                        &part.normalized_id,
                        output_index,
                        part.observed.byte_len(),
                        part.observed.sha256(),
                        limits,
                    )?),
                )
            }
        };
        if let Some(cursor) = completion {
            self.stage_completion(cursor)?;
        }
        Ok(marker.into_iter().collect())
    }

    pub(super) fn content_part_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (wire_id, output_index, content_index) = content_coordinates(value)?;
        let normalized = {
            let part = self
                .state
                .part_mut(wire_id, content_index)
                .ok_or_else(|| error::malformed("OpenAI content-part done preceded start"))?;
            if part.output_index != output_index || !part.value_done || part.completed {
                return Err(error::malformed("OpenAI content-part terminal was inconsistent"));
            }
            part.completed = true;
            part.normalized_id.clone()
        };
        if !self.state.record_part_completion(wire_id) {
            return Err(error::malformed("OpenAI content completion accounting changed"));
        }
        Ok(vec![ModelEvent::ItemCompleted(normalized)])
    }

    pub(super) fn tool_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let wire_id = string_field(value, "item_id")?;
        let output_index = u32_field(value, "output_index")?;
        let delta = string_field_allow_empty(value, "delta")?.as_bytes();
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
        item.arguments.append(delta, limits)?;
        if delta.is_empty() {
            return Self::ancillary(value);
        }
        item.argument_progress_revision = item
            .argument_progress_revision
            .checked_add(1)
            .ok_or_else(|| error::limit("OpenAI tool progress revision overflowed"))?;
        let call_id = item
            .call_id
            .as_ref()
            .ok_or_else(|| error::malformed("OpenAI tool item omitted its call identity"))?;
        let fragment = StreamFragment::new(delta.to_vec(), limits)
            .map_err(|_| error::limit("OpenAI tool fragment exceeds protocol limits"))?;
        Ok(vec![ModelEvent::ToolArgumentProgress {
            call_id: call_id.clone(),
            revision: item.argument_progress_revision,
            fragment,
        }])
    }

    pub(super) fn tool_done(
        &mut self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let wire_id = string_field(value, "item_id")?;
        let output_index = u32_field(value, "output_index")?;
        let arguments = string_field_any(value, &["arguments", "input"])?;
        let limits = self.limits;
        let cursor = {
            let item = self
                .state
                .item_mut(wire_id)
                .ok_or_else(|| error::malformed("OpenAI tool done preceded its item"))?;
            if item.kind != ItemKind::ToolCall
                || item.output_index != output_index
                || item.arguments_done
                || item.arguments.as_bytes() != arguments.as_bytes()
            {
                return Err(error::malformed("OpenAI finalized tool input contradicted its deltas"));
            }
            if let Some(name) = value.get("name").and_then(Value::as_str)
                && item.call_name.as_ref().is_none_or(|known| known.as_str() != name)
            {
                return Err(error::malformed("OpenAI finalized tool name changed"));
            }
            item.arguments_done = true;
            let call_id = item
                .call_id
                .clone()
                .ok_or_else(|| error::malformed("OpenAI tool item omitted its call identity"))?;
            core::mem::take(&mut item.arguments).into_completion(call_id, limits)?
        };
        self.stage_completion(cursor)?;
        Ok(Vec::new())
    }

    pub(super) fn reasoning_delta(
        &self,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let wire_id = string_field(value, "item_id")?;
        let output_index = u32_field(value, "output_index")?;
        let delta = string_field_allow_empty(value, "delta")?.as_bytes();
        let item = self
            .state
            .item(wire_id)
            .ok_or_else(|| error::malformed("OpenAI reasoning delta preceded its item"))?;
        if item.kind != ItemKind::Reasoning || item.output_index != output_index || item.completed {
            return Err(error::malformed("OpenAI reasoning delta targeted an incompatible item"));
        }
        if delta.is_empty() {
            return Self::ancillary(value);
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
                    if encrypted.len() > limits.max_extension_bytes() {
                        return Err(error::limit(
                            "OpenAI reasoning replay exceeds its extension bound",
                        ));
                    }
                    if !encrypted.is_empty() {
                        let fragment = StreamFragment::new(encrypted.as_bytes().to_vec(), limits)
                            .map_err(|_| error::limit("OpenAI reasoning replay exceeds limits"))?;
                        events.push(ModelEvent::ReasoningReplayDelta {
                            item_id: item.normalized_id.clone(),
                            fragment,
                        });
                    }
                }
                events.push(ModelEvent::ItemCompleted(item.normalized_id.clone()));
            }
            ItemKind::ProviderNative | ItemKind::ToolCall => {
                events.push(ModelEvent::ItemCompleted(item.normalized_id.clone()));
            }
            ItemKind::Message => {
                events.push(message_done_marker(
                    &item.normalized_id,
                    output_index,
                    limits,
                )?);
            }
            ItemKind::StructuredOutput | ItemKind::Refusal => {
                return Err(error::malformed("invalid OpenAI output item state"));
            }
        }
        item.completed = true;
        Ok(events)
    }

    fn claim_item_id(
        &mut self,
        wire_id: &str,
        suffix: &str,
        preferred: ItemId,
    ) -> Result<ItemId, ProviderCoreError> {
        if self.state.claim_normalized_id(preferred.clone()) {
            return Ok(preferred);
        }
        let derived = identity::derived_item_id(wire_id, suffix)?;
        if derived == preferred || !self.state.claim_normalized_id(derived.clone()) {
            return Err(error::malformed("OpenAI normalized item identity collided"));
        }
        Ok(derived)
    }
}

fn content_coordinates(value: &Value) -> Result<(&str, u32, u32), ProviderCoreError> {
    Ok((
        string_field(value, "item_id")?,
        u32_field(value, "output_index")?,
        u32_field(value, "content_index")?,
    ))
}

fn content_done_marker(
    item_id: &ItemId,
    output_index: u32,
    observed_bytes: usize,
    observed_digest: peritus_types::Sha256Digest,
    limits: ProtocolLimits,
) -> Result<ModelEvent, ProviderCoreError> {
    let observed_bytes = u64::try_from(observed_bytes)
        .map_err(|_| error::limit("OpenAI observed content length exceeded u64"))?;
    state_marker(
        serde_json::json!({
            "type": "content_done_v1",
            "item_id": item_id.expose_for_wire(),
            "output_index": output_index,
            "observed_bytes": observed_bytes,
            "observed_sha256": observed_digest.as_bytes(),
        }),
        limits,
    )
}

fn message_done_marker(
    item_id: &ItemId,
    output_index: u32,
    limits: ProtocolLimits,
) -> Result<ModelEvent, ProviderCoreError> {
    state_marker(
        serde_json::json!({
            "type": "message_done_v1",
            "item_id": item_id.expose_for_wire(),
            "output_index": output_index,
        }),
        limits,
    )
}

fn state_marker(value: Value, limits: ProtocolLimits) -> Result<ModelEvent, ProviderCoreError> {
    let serialized = serde_json::to_string(&value)
        .map_err(|_| error::malformed("OpenAI state marker serialization failed"))?;
    let value = CanonicalJson::parse(&serialized, JsonBounds::extension(limits))
        .map_err(|_| error::limit("OpenAI state marker exceeded protocol bounds"))?;
    let name = ExtensionName::new("openai.state".to_owned())
        .map_err(|_| error::malformed("static OpenAI state extension identity was invalid"))?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
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

fn string_field_allow_empty<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
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
