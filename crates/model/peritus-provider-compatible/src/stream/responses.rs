mod terminal;
use peritus_model_protocol::{
    EventId, ItemId, ItemKind, ModelEvent, ModelName, ProtocolLimits, ProviderName, ResponseId,
    StreamFragment, ToolCallId, ToolName,
};
use peritus_provider_core::{ProviderCoreError, SseFrame};
use serde_json::Value;

use super::ancillary;
use super::responses_state::{ItemState, PartState, ResponsesState, SequenceDisposition};
use crate::error;

pub(super) struct ResponsesDecoder {
    provider: ProviderName,
    model: ModelName,
    structured: bool,
    allow_tools: bool,
    allow_usage: bool,
    limits: ProtocolLimits,
    state: ResponsesState,
}

pub(super) struct FrameEvents {
    pub provider_sequence: Option<u64>,
    pub provider_event_id: Option<EventId>,
    pub digest: peritus_types::Sha256Digest,
    pub events: Vec<ModelEvent>,
}

impl ResponsesDecoder {
    pub const fn new(
        provider: ProviderName,
        model: ModelName,
        structured: bool,
        allow_tools: bool,
        allow_usage: bool,
        limits: ProtocolLimits,
    ) -> Self {
        Self {
            provider,
            model,
            structured,
            allow_tools,
            allow_usage,
            limits,
            state: ResponsesState::new(),
        }
    }

    pub const fn response_id(&self) -> Option<&ResponseId> {
        self.state.response_id()
    }

    pub fn decode(&mut self, frame: &SseFrame) -> Result<FrameEvents, ProviderCoreError> {
        if frame.data().len() > self.limits.max_event_bytes() {
            return Err(error::limit("Responses-compatible event exceeded its byte bound"));
        }
        let value: Value = serde_json::from_str(frame.data())
            .map_err(|_| error::malformed("Responses-compatible event was not JSON"))?;
        if !value.is_object() {
            return Err(error::malformed("Responses-compatible event was not an object"));
        }
        let kind = string(&value, "type")?;
        if frame.event().is_some_and(|value| value != kind) {
            return Err(error::malformed("Responses-compatible SSE and JSON event types differ"));
        }
        let sequence = integer(&value, "sequence_number")?;
        let digest = peritus_codec::sha256(frame.data().as_bytes());
        let identity = EventId::new(
            frame
                .id()
                .filter(|value| !value.is_empty())
                .map_or_else(|| format!("compatible-response-{sequence}"), str::to_owned),
        )
        .map_err(|_| error::malformed("Responses-compatible event identity was invalid"))?;
        let events = match self.state.sequence(sequence, digest) {
            SequenceDisposition::Duplicate => vec![ModelEvent::Heartbeat],
            SequenceDisposition::Conflict => {
                return Err(error::malformed("Responses-compatible sequence was reordered"));
            }
            SequenceDisposition::New => self.map(kind, &value)?,
        };
        Ok(FrameEvents {
            provider_sequence: Some(sequence),
            provider_event_id: Some(identity),
            digest,
            events,
        })
    }

    pub const fn done() -> Result<Vec<ModelEvent>, ProviderCoreError> {
        Err(error::malformed(
            "Responses-compatible DONE did not replace an explicit terminal event",
        ))
    }

    fn map(&mut self, kind: &str, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        match kind {
            "response.created" => self.created(value),
            "response.queued" | "response.in_progress" => self.lifecycle(value),
            "response.output_item.added" => self.item_added(value),
            "response.content_part.added" => self.part_added(value),
            "response.output_text.delta" => self.part_delta(value, false),
            "response.refusal.delta" => self.part_delta(value, true),
            "response.output_text.done" => self.part_value_done(value, false),
            "response.refusal.done" => self.part_value_done(value, true),
            "response.content_part.done" => self.part_done(value),
            "response.function_call_arguments.delta" => self.tool_delta(value),
            "response.function_call_arguments.done" => self.tool_done(value),
            "response.output_item.done" => self.item_done(value),
            "response.completed" => self.completed(value),
            "response.failed" => self.failed(value, false),
            "response.incomplete" => self.failed(value, true),
            "error" => self.stream_error(),
            unknown if ancillary::safe_responses(unknown) => {
                Ok(vec![ancillary::event(value, self.limits)?])
            }
            _ => Err(error::malformed("unknown correctness-critical Responses-compatible event")),
        }
    }

    fn created(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response = object(value, "response")?;
        let id = ResponseId::new(string(response, "id")?.to_owned())
            .map_err(|_| error::malformed("Responses-compatible identity was invalid"))?;
        let model = ModelName::new(string(response, "model")?.to_owned())
            .map_err(|_| error::malformed("Responses-compatible model was invalid"))?;
        if model != self.model
            || !matches!(string(response, "status")?, "queued" | "in_progress")
            || !self.state.start(id.clone())
        {
            return Err(error::malformed("Responses-compatible creation contradicted request"));
        }
        Ok(vec![ModelEvent::ResponseStarted { response_id: Some(id), model: Some(model) }])
    }

    fn lifecycle(&self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let response = object(value, "response")?;
        if !self.state.response_matches(string(response, "id")?) {
            return Err(error::malformed("Responses-compatible lifecycle identity changed"));
        }
        Ok(vec![ancillary::event(value, self.limits)?])
    }

    fn item_added(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let index = index(value, "output_index")?;
        let item = object(value, "item")?;
        let wire_id = string(item, "id")?;
        let normalized = ItemId::new(wire_id.to_owned())
            .map_err(|_| error::malformed("Responses-compatible item identity was invalid"))?;
        let kind = match string(item, "type")? {
            "message" => ItemKind::Message,
            "function_call" if self.allow_tools => ItemKind::ToolCall,
            _ => return Err(error::malformed("unmapped Responses-compatible output item")),
        };
        let mut events = Vec::new();
        let call_id = if kind == ItemKind::ToolCall {
            let call_id = ToolCallId::new(string(item, "call_id")?.to_owned())
                .map_err(|_| error::malformed("compatible tool-call identity was invalid"))?;
            let name = ToolName::new(string(item, "name")?.to_owned())
                .map_err(|_| error::malformed("compatible tool name was invalid"))?;
            events.push(ModelEvent::ItemStarted { item_id: normalized.clone(), index, kind });
            events.push(ModelEvent::ToolCallStarted {
                item_id: normalized.clone(),
                call_id: call_id.clone(),
                name,
            });
            Some(call_id)
        } else {
            None
        };
        if !self.state.insert_item(
            wire_id.to_owned(),
            ItemState {
                normalized,
                index,
                kind,
                call_id,
                bytes: Vec::new(),
                value_done: false,
                completed: false,
            },
        ) {
            return Err(error::malformed("Responses-compatible item was duplicated"));
        }
        Ok(events)
    }

    fn part_added(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (id, output, content) = coordinates(value)?;
        let item = self.state.item(id).ok_or_else(|| {
            error::malformed("Responses-compatible content preceded its message item")
        })?;
        if item.kind != ItemKind::Message || item.index != output || item.completed {
            return Err(error::malformed("Responses-compatible content targeted wrong item"));
        }
        let kind = match string(object(value, "part")?, "type")? {
            "output_text" if self.structured => ItemKind::StructuredOutput,
            "output_text" => ItemKind::Message,
            "refusal" => ItemKind::Refusal,
            _ => return Err(error::malformed("unmapped Responses-compatible content part")),
        };
        let normalized = part_id(id, content)?;
        if !self.state.insert_part(
            id.to_owned(),
            content,
            PartState {
                normalized: normalized.clone(),
                index: output,
                kind,
                bytes: Vec::new(),
                value_done: false,
                completed: false,
            },
        ) {
            return Err(error::malformed("Responses-compatible content part was duplicated"));
        }
        Ok(vec![ModelEvent::ItemStarted {
            item_id: normalized,
            index: normalized_index(output, content)?,
            kind,
        }])
    }

    fn part_delta(
        &mut self,
        value: &Value,
        refusal: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (id, output, content) = coordinates(value)?;
        let bytes = string(value, "delta")?.as_bytes();
        let part = self.state.part_mut(id, content).ok_or_else(|| {
            error::malformed("Responses-compatible delta preceded its content part")
        })?;
        if part.index != output
            || part.value_done
            || part.completed
            || refusal != (part.kind == ItemKind::Refusal)
        {
            return Err(error::malformed("Responses-compatible delta targeted wrong part"));
        }
        append(&mut part.bytes, bytes, self.limits.max_output_bytes())?;
        let fragment = StreamFragment::new(bytes.to_vec(), self.limits)
            .map_err(|_| error::limit("compatible content fragment exceeded bounds"))?;
        Ok(vec![if refusal {
            ModelEvent::RefusalDelta { item_id: part.normalized.clone(), fragment }
        } else {
            ModelEvent::TextDelta { item_id: part.normalized.clone(), fragment }
        }])
    }

    fn part_value_done(
        &mut self,
        value: &Value,
        refusal: bool,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (id, output, content) = coordinates(value)?;
        let name = if refusal { "refusal" } else { "text" };
        let complete = string(value, name)?.as_bytes();
        let part = self.state.part_mut(id, content).ok_or_else(|| {
            error::malformed("Responses-compatible content done preceded its part")
        })?;
        if part.index != output
            || part.value_done
            || part.bytes != complete
            || refusal != (part.kind == ItemKind::Refusal)
        {
            return Err(error::malformed("Responses-compatible completed content changed"));
        }
        part.value_done = true;
        Ok(Vec::new())
    }

    fn part_done(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let (id, output, content) = coordinates(value)?;
        let part = self.state.part_mut(id, content).ok_or_else(|| {
            error::malformed("Responses-compatible content terminal preceded its part")
        })?;
        if part.index != output || !part.value_done || part.completed {
            return Err(error::malformed("Responses-compatible content terminal was invalid"));
        }
        part.completed = true;
        Ok(vec![ModelEvent::ItemCompleted(part.normalized.clone())])
    }

    fn tool_delta(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let id = string(value, "item_id")?;
        let index = index(value, "output_index")?;
        let bytes = string(value, "delta")?.as_bytes();
        let item = self
            .state
            .item_mut(id)
            .ok_or_else(|| error::malformed("Responses-compatible tool delta preceded its item"))?;
        if item.kind != ItemKind::ToolCall
            || item.index != index
            || item.value_done
            || item.completed
        {
            return Err(error::malformed("Responses-compatible tool delta targeted wrong item"));
        }
        append(&mut item.bytes, bytes, self.limits.max_tool_argument_bytes())?;
        let call_id = item.call_id.clone().ok_or_else(|| {
            error::malformed("Responses-compatible tool item omitted call identity")
        })?;
        let fragment = StreamFragment::new(bytes.to_vec(), self.limits)
            .map_err(|_| error::limit("compatible tool fragment exceeded bounds"))?;
        Ok(vec![ModelEvent::ToolArgumentDelta { call_id, fragment }])
    }

    fn tool_done(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let id = string(value, "item_id")?;
        let index = index(value, "output_index")?;
        let complete = string(value, "arguments")?.as_bytes();
        let item = self.state.item_mut(id).ok_or_else(|| {
            error::malformed("Responses-compatible tool terminal preceded its item")
        })?;
        if item.kind != ItemKind::ToolCall
            || item.index != index
            || item.value_done
            || item.bytes != complete
        {
            return Err(error::malformed("Responses-compatible completed tool input changed"));
        }
        item.value_done = true;
        Ok(Vec::new())
    }

    fn item_done(&mut self, value: &Value) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let output = index(value, "output_index")?;
        let id = string(object(value, "item")?, "id")?;
        let parts_complete = self.state.parts_complete(id);
        let item = self.state.item_mut(id).ok_or_else(|| {
            error::malformed("Responses-compatible item terminal preceded its item")
        })?;
        if item.index != output
            || item.completed
            || item.kind == ItemKind::ToolCall && !item.value_done
            || item.kind == ItemKind::Message && !parts_complete
        {
            return Err(error::malformed("Responses-compatible item terminal was invalid"));
        }
        item.completed = true;
        Ok(if item.kind == ItemKind::ToolCall {
            vec![ModelEvent::ItemCompleted(item.normalized.clone())]
        } else {
            Vec::new()
        })
    }
}

pub(super) fn object<'a>(value: &'a Value, name: &str) -> Result<&'a Value, ProviderCoreError> {
    value
        .get(name)
        .filter(|value| value.is_object())
        .ok_or_else(|| error::malformed("Responses-compatible event omitted a required object"))
}

pub(super) fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error::malformed("Responses-compatible event omitted a required string"))
}

fn integer(value: &Value, name: &str) -> Result<u64, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| error::malformed("Responses-compatible event omitted a required integer"))
}

fn index(value: &Value, name: &str) -> Result<u32, ProviderCoreError> {
    u32::try_from(integer(value, name)?)
        .map_err(|_| error::malformed("Responses-compatible index exceeded u32"))
}

fn coordinates(value: &Value) -> Result<(&str, u32, u32), ProviderCoreError> {
    Ok((string(value, "item_id")?, index(value, "output_index")?, index(value, "content_index")?))
}

fn part_id(item: &str, content: u32) -> Result<ItemId, ProviderCoreError> {
    ItemId::new(if content == 0 { item.to_owned() } else { format!("{item}-part-{content}") })
        .map_err(|_| error::malformed("compatible normalized item identity was invalid"))
}

fn normalized_index(output: u32, content: u32) -> Result<u32, ProviderCoreError> {
    output
        .checked_mul(65_536)
        .and_then(|value| value.checked_add(content))
        .ok_or_else(|| error::limit("Responses-compatible item/content index overflowed"))
}

fn append(target: &mut Vec<u8>, value: &[u8], maximum: usize) -> Result<(), ProviderCoreError> {
    if target.len().checked_add(value.len()).is_none_or(|length| length > maximum) {
        return Err(error::limit("compatible fragmented output exceeded aggregate bounds"));
    }
    target.extend_from_slice(value);
    Ok(())
}
