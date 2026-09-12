mod fields;
mod hosted;
mod tools;
use tools::ToolState;

use std::collections::BTreeMap;

use peritus_model_protocol::{
    EventId, FinishReason, ItemId, ItemKind, ModelEvent, ModelName, ProtocolLimits, ProviderName,
    ResponseId, StreamFragment, UsageCounters, UsageObservation, UsageScope,
};
use peritus_provider_core::{ProviderCoreError, SseFrame};
use serde_json::{Map, Value};

use super::responses::FrameEvents;
use crate::error;
use fields::{append, integer, string, validate_top_level};

pub(super) struct ChatDecoder {
    pub(super) service: Option<peritus_provider_core::hosted::HostedService>,
    provider: ProviderName,
    actual_model: Option<ModelName>,
    reasoning: Map<String, Value>,
    pub(super) tool_choice: peritus_model_protocol::ToolChoice,
    expected_model: ModelName,
    structured: bool,
    allow_tools: bool,
    allow_usage: bool,
    limits: ProtocolLimits,
    response_id: Option<ResponseId>,
    text: Option<ItemId>,
    text_bytes: Vec<u8>,
    refusal: Option<ItemId>,
    refusal_bytes: Vec<u8>,
    tools: BTreeMap<u32, ToolState>,
    finish: Option<hosted::CompletedChoice>,
    usage: Option<Box<UsageCounters>>,
}

impl ChatDecoder {
    pub fn new(
        provider: ProviderName,
        expected_model: ModelName,
        structured: bool,
        allow_tools: bool,
        allow_usage: bool,
        limits: ProtocolLimits,
    ) -> Self {
        Self {
            service: None,
            provider,
            actual_model: None,
            reasoning: Map::new(),
            tool_choice: peritus_model_protocol::ToolChoice::Auto,
            expected_model,
            structured,
            allow_tools,
            allow_usage,
            limits,
            response_id: None,
            text: None,
            text_bytes: Vec::new(),
            refusal: None,
            refusal_bytes: Vec::new(),
            tools: BTreeMap::new(),
            finish: None,
            usage: None,
        }
    }

    pub const fn response_id(&self) -> Option<&ResponseId> {
        self.response_id.as_ref()
    }

    pub fn decode(&mut self, frame: &SseFrame) -> Result<FrameEvents, ProviderCoreError> {
        if frame.data().len() > self.limits.max_event_bytes() {
            return Err(error::limit("Chat-compatible chunk exceeded its event bound"));
        }
        if frame.event().is_some_and(|event| event != "message") {
            return Err(error::malformed("Chat-compatible SSE event name was unsupported"));
        }
        let value: Value = serde_json::from_str(frame.data())
            .map_err(|_| error::malformed("Chat-compatible chunk was not JSON"))?;
        let object = value
            .as_object()
            .ok_or_else(|| error::malformed("Chat-compatible chunk was not a JSON object"))?;
        validate_top_level(object, self.service)?;
        if self.service == Some(peritus_provider_core::hosted::HostedService::OpenRouter)
            && let Some(failure) = object.get("error").filter(|value| !value.is_null())
        {
            return self.provider_failure(failure, frame);
        }
        if string(&value, "object")? != "chat.completion.chunk" {
            return Err(error::malformed("Chat-compatible object discriminator was unknown"));
        }
        let id = ResponseId::new(string(&value, "id")?.to_owned())
            .map_err(|_| error::malformed("Chat-compatible response identity was invalid"))?;
        let model = ModelName::new(string(&value, "model")?.to_owned())
            .map_err(|_| error::malformed("Chat-compatible model identity was invalid"))?;
        if self.service.is_none() && model != self.expected_model
            || self.actual_model.as_ref().is_some_and(|known| known != &model)
            || self.response_id.as_ref().is_some_and(|known| known != &id)
        {
            return Err(error::malformed("Chat-compatible chunk identity or model changed"));
        }
        let digest = peritus_codec::sha256(frame.data().as_bytes());
        let event_id = frame
            .id()
            .filter(|value| !value.is_empty())
            .map(|value| EventId::new(value.to_owned()))
            .transpose()
            .map_err(|_| error::malformed("Chat-compatible SSE event identity was invalid"))?;
        let mut events = Vec::new();
        if self.response_id.is_none() {
            self.actual_model = Some(model.clone());
            self.response_id = Some(id.clone());
            events.push(ModelEvent::ResponseStarted { response_id: Some(id), model: Some(model) });
        }
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| error::malformed("Chat-compatible chunk omitted choices"))?;
        if choices.len() > 1 {
            return Err(error::malformed("Chat-compatible multiple choices are not mapped"));
        }
        if let Some(choice) = choices.first() {
            let accounting = self.service
                == Some(peritus_provider_core::hosted::HostedService::OpenRouter)
                && self.finish.is_some()
                && value.get("usage").is_some_and(|value| !value.is_null());
            if accounting {
                self.accounting(choice)?;
            } else {
                self.choice(choice, &mut events)?;
            }
        }
        if let Some(usage) = value.get("usage").filter(|value| !value.is_null()) {
            if !self.allow_usage {
                return Err(error::malformed(
                    "Chat-compatible usage was not declared by the profile",
                ));
            }
            self.observe_usage(usage, &mut events)?;
        }
        if value.get("provider_metadata").is_some() {
            let metadata = value
                .get("provider_metadata")
                .ok_or_else(|| error::malformed("Chat-compatible provider metadata disappeared"))?;
            events.push(super::ancillary::event(metadata, self.limits)?);
        }
        if self.service == Some(peritus_provider_core::hosted::HostedService::Groq)
            && let Some(metadata) = value.get("x_groq")
        {
            if let Some(usage) = metadata.get("usage").filter(|value| !value.is_null())
                && self.allow_usage
            {
                self.observe_usage(usage, &mut events)?;
            }
            events.push(super::ancillary::event(
                &serde_json::json!({"x_groq":metadata}),
                self.limits,
            )?);
        }
        if self.service == Some(peritus_provider_core::hosted::HostedService::OpenRouter)
            && let Some(provider) = value.get("provider")
        {
            events.push(super::ancillary::event(
                &serde_json::json!({"provider":provider}),
                self.limits,
            )?);
        }
        if events.is_empty() {
            events.push(ModelEvent::Heartbeat);
        }
        Ok(FrameEvents { provider_sequence: None, provider_event_id: event_id, digest, events })
    }

    pub fn done(&self) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        if self.finish.is_none()
            || self.response_id.is_none()
            || self.tools.values().any(|v| !v.completed)
        {
            return Err(error::malformed("Chat-compatible DONE preceded a mapped finish"));
        }
        let mut events = Vec::with_capacity(usize::from(self.usage.is_some()) + 1);
        if let Some(usage) = self.usage.as_deref() {
            events.push(ModelEvent::Usage(UsageObservation::new(UsageScope::Final, *usage, None)));
        }
        events.push(ModelEvent::ResponseCompleted);
        Ok(events)
    }

    fn observe_usage(
        &mut self,
        value: &Value,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        let observation = fields::usage(value)?;
        let counters = observation.counters();
        if let Some(usage) = self.usage.as_deref_mut() {
            *usage = counters;
        } else {
            self.usage = Some(Box::new(counters));
        }
        events.push(ModelEvent::Usage(observation));
        Ok(())
    }

    fn choice(
        &mut self,
        choice: &Value,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        fields::validate_choice(choice, self.service)?;
        if integer(choice, "index")? != 0 || self.finish.is_some() {
            return Err(error::malformed("Chat-compatible choice index or lifecycle was invalid"));
        }
        let delta = choice
            .get("delta")
            .and_then(Value::as_object)
            .ok_or_else(|| error::malformed("Chat-compatible choice omitted delta"))?;
        self.delta(delta, events)?;
        match choice.get("finish_reason") {
            None | Some(Value::Null) => {}
            Some(Value::String(reason)) => self.finish(reason, events)?,
            Some(_) => return Err(error::malformed("Chat-compatible finish reason was invalid")),
        }
        Ok(())
    }

    fn delta(
        &mut self,
        delta: &Map<String, Value>,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        for name in delta.keys() {
            if !matches!(name.as_str(), "role" | "content" | "refusal" | "tool_calls")
                && !self
                    .service
                    .is_some_and(|service| crate::hosted_reasoning::accepts(service, name))
            {
                return Err(error::malformed("Chat-compatible delta field was unmapped"));
            }
        }
        if let Some(role) = delta.get("role")
            && role.as_str() != Some("assistant")
        {
            return Err(error::malformed("Chat-compatible delta role was not assistant"));
        }
        for (name, value) in delta {
            if self.service.is_some_and(|service| crate::hosted_reasoning::accepts(service, name)) {
                crate::hosted_reasoning::append(
                    &mut self.reasoning,
                    name,
                    value,
                    self.limits.max_output_bytes(),
                )?;
            }
        }
        if let Some(content) =
            delta.get("content").filter(|value| !value.is_null() && value.as_str() != Some(""))
        {
            let content = content.as_str().ok_or_else(|| {
                error::malformed("Chat-compatible content delta was not a string")
            })?;
            let item = self.ensure_text(events)?;
            append(&mut self.text_bytes, content.as_bytes(), self.limits.max_output_bytes())?;
            if !self.structured {
                let fragment = StreamFragment::new(content.as_bytes().to_vec(), self.limits)
                    .map_err(|_| {
                        error::limit("Chat-compatible content fragment exceeded bounds")
                    })?;
                events.push(ModelEvent::TextDelta { item_id: item, fragment });
            }
        }
        if let Some(refusal) = delta.get("refusal").filter(|value| !value.is_null()) {
            let refusal = refusal.as_str().filter(|value| !value.is_empty()).ok_or_else(|| {
                error::malformed("Chat-compatible refusal delta was not a nonempty string")
            })?;
            let item = self.ensure_refusal(events)?;
            append(&mut self.refusal_bytes, refusal.as_bytes(), self.limits.max_output_bytes())?;
            let fragment = StreamFragment::new(refusal.as_bytes().to_vec(), self.limits)
                .map_err(|_| error::limit("Chat-compatible refusal fragment exceeded bounds"))?;
            events.push(ModelEvent::RefusalDelta { item_id: item, fragment });
        }
        if let Some(tools) = delta.get("tool_calls") {
            if !self.allow_tools {
                return Err(error::malformed(
                    "Chat-compatible tools were not declared by the profile",
                ));
            }
            let tools = tools.as_array().ok_or_else(|| {
                error::malformed("Chat-compatible tool-call delta was not an array")
            })?;
            for tool in tools {
                self.tool(tool, events)?;
            }
        }
        Ok(())
    }

    fn ensure_text(&mut self, events: &mut Vec<ModelEvent>) -> Result<ItemId, ProviderCoreError> {
        if self.refusal.is_some() {
            return Err(error::malformed(
                "Chat-compatible output mixed refusal and assistant text",
            ));
        }
        if let Some(item) = &self.text {
            return Ok(item.clone());
        }
        let response = self
            .response_id
            .as_ref()
            .ok_or_else(|| error::malformed("Chat-compatible response identity was unavailable"))?;
        let item =
            ItemId::new(format!("{}-message", response.expose_for_wire())).map_err(|_| {
                error::malformed("Chat-compatible normalized item identity was invalid")
            })?;
        events.push(ModelEvent::ItemStarted {
            item_id: item.clone(),
            index: 0,
            kind: if self.structured { ItemKind::StructuredOutput } else { ItemKind::Message },
        });
        self.text = Some(item.clone());
        Ok(item)
    }

    fn ensure_refusal(
        &mut self,
        events: &mut Vec<ModelEvent>,
    ) -> Result<ItemId, ProviderCoreError> {
        if self.text.is_some() || !self.tools.is_empty() {
            return Err(error::malformed(
                "Chat-compatible refusal mixed with another output family",
            ));
        }
        if let Some(item) = &self.refusal {
            return Ok(item.clone());
        }
        let response = self
            .response_id
            .as_ref()
            .ok_or_else(|| error::malformed("Chat-compatible response identity was unavailable"))?;
        let item = ItemId::new(format!("{}-refusal", response.expose_for_wire()))
            .map_err(|_| error::malformed("Chat-compatible refusal identity was invalid"))?;
        events.push(ModelEvent::ItemStarted {
            item_id: item.clone(),
            index: 1,
            kind: ItemKind::Refusal,
        });
        self.refusal = Some(item.clone());
        Ok(item)
    }

    fn finish(
        &mut self,
        value: &str,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        let reason = match value {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" => FinishReason::ToolCalls,
            "content_filter" => FinishReason::Safety,
            _ => return Err(error::malformed("Chat-compatible finish reason was unmapped")),
        };
        self.validate_tool_choice()?;
        if let Some(item) = &self.text {
            if self.structured {
                events.extend(peritus_provider_core::healing::structured_output(
                    &self.text_bytes,
                    item,
                    self.limits,
                )?);
            }
            events.push(ModelEvent::ItemCompleted(item.clone()));
        }
        if let Some(item) = &self.refusal {
            events.push(ModelEvent::ItemCompleted(item.clone()));
        }
        for state in self.tools.values_mut() {
            if state.completed {
                return Err(error::malformed("Chat-compatible tool item completed twice"));
            }
            state.completed = true;
            events.extend(state.bytes.complete(&state.call_id, self.limits)?);
            events.push(ModelEvent::ItemCompleted(state.item_id.clone()));
        }
        self.finish_reasoning(events)?;
        self.finish =
            Some(hosted::CompletedChoice { wire_reason: value.to_owned(), accounting_seen: false });
        events.push(ModelEvent::Finish(reason));
        Ok(())
    }
}
