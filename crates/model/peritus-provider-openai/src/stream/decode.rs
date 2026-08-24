//! Bounded event-envelope parsing, sequence validation, and event dispatch.

use peritus_model_protocol::{CanonicalJson, EventId, JsonBounds, ModelEvent, ProtocolLimits};
use peritus_provider_core::{ProviderCoreError, SseFrame};
use serde_json::Value;

use super::{OpenAiStream, state::SequenceDisposition};
use crate::error;

impl OpenAiStream {
    pub(super) fn decode_frame(&mut self, frame: &SseFrame) -> Result<(), ProviderCoreError> {
        if frame.data().len() > self.limits.max_event_bytes() {
            return Err(error::limit("OpenAI SSE event exceeds the protocol event bound"));
        }
        CanonicalJson::parse(frame.data(), JsonBounds::value(ProtocolLimits::PRODUCTION)).map_err(
            |_| error::malformed("OpenAI SSE data is malformed or recursively unbounded"),
        )?;
        let value: Value = serde_json::from_str(frame.data())
            .map_err(|_| error::malformed("OpenAI SSE data is not a JSON object"))?;
        if !value.is_object() {
            return Err(error::malformed("OpenAI SSE data is not a JSON object"));
        }
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| error::malformed("OpenAI SSE event omitted its type"))?;
        if frame.event().is_some_and(|name| name != event_type) {
            return Err(error::malformed("OpenAI SSE event name contradicted its JSON type"));
        }
        let provider_sequence = value
            .get("sequence_number")
            .and_then(Value::as_u64)
            .ok_or_else(|| error::malformed("OpenAI SSE event omitted its sequence number"))?;
        let digest = peritus_codec::sha256(frame.data().as_bytes());
        let event_id = event_identity(frame, provider_sequence)?;
        match self.state.observe_sequence(provider_sequence, digest) {
            SequenceDisposition::Conflict => {
                return Err(error::malformed("OpenAI provider sequence was reordered or reused"));
            }
            SequenceDisposition::Duplicate => {
                self.enqueue(
                    Some(provider_sequence),
                    Some(event_id),
                    digest,
                    ModelEvent::Heartbeat,
                )?;
                return Ok(());
            }
            SequenceDisposition::New => {}
        }
        let mut events = self.map_event(event_type, &value)?;
        if events.is_empty() {
            events.push(ModelEvent::Heartbeat);
        }
        for (index, event) in events.into_iter().enumerate() {
            self.enqueue(
                (index == 0).then_some(provider_sequence),
                (index == 0).then(|| event_id.clone()),
                digest,
                event,
            )?;
        }
        Ok(())
    }

    fn map_event(
        &mut self,
        event_type: &str,
        value: &Value,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        match event_type {
            "response.created" => self.response_created(value),
            "response.queued" | "response.in_progress" => {
                self.lifecycle_observation(value)?;
                Self::ancillary(value)
            }
            "response.output_item.added" => self.output_item_added(value),
            "response.output_item.done" => self.output_item_done(value),
            "response.content_part.added" => self.content_part_added(value),
            "response.content_part.done" => self.content_part_done(value),
            "response.output_text.delta" => self.content_delta(value, false),
            "response.output_text.done" => self.content_value_done(value, false),
            "response.refusal.delta" => self.content_delta(value, true),
            "response.refusal.done" => self.content_value_done(value, true),
            "response.function_call_arguments.delta" | "response.custom_tool_call_input.delta" => {
                self.tool_delta(value)
            }
            "response.function_call_arguments.done" | "response.custom_tool_call_input.done" => {
                self.tool_done(value)
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                self.reasoning_delta(value)
            }
            "response.reasoning_summary_text.done"
            | "response.reasoning_text.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
            | "response.output_text.annotation.added" => Self::ancillary(value),
            "response.completed" => self.response_completed(value),
            "response.failed" => self.response_failed(value, false),
            "response.incomplete" => self.response_failed(value, true),
            "error" => self.stream_error(value),
            ancillary if safe_ancillary(ancillary) => Self::ancillary(value),
            _ => Err(error::malformed("unknown correctness-critical OpenAI streaming event")),
        }
    }

    fn lifecycle_observation(&self, value: &Value) -> Result<(), ProviderCoreError> {
        let response = value
            .get("response")
            .filter(|value| value.is_object())
            .ok_or_else(|| error::malformed("OpenAI lifecycle event omitted its response"))?;
        let identity = response
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| error::malformed("OpenAI lifecycle event omitted response identity"))?;
        if !self.state.response_matches(identity) {
            return Err(error::malformed("OpenAI lifecycle response identity changed"));
        }
        Ok(())
    }
}

fn event_identity(frame: &SseFrame, sequence: u64) -> Result<EventId, ProviderCoreError> {
    let value = frame
        .id()
        .filter(|value| !value.is_empty())
        .map_or_else(|| format!("openai-sequence-{sequence}"), str::to_owned);
    EventId::new(value).map_err(|_| error::malformed("OpenAI SSE event identity is invalid"))
}

fn safe_ancillary(event_type: &str) -> bool {
    if !event_type.starts_with("response.") {
        return true;
    }
    let segments = event_type.split('.').count();
    segments > 2
        && (event_type.ends_with(".in_progress")
            || event_type.ends_with(".searching")
            || event_type.ends_with(".queued")
            || event_type.ends_with(".completed"))
}
