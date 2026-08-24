//! Stable-v1 Interactions event grammar and step normalization.

use peritus_model_protocol::{
    CanonicalJson, FailureCategory, FinishReason, ItemId, ItemKind, JsonBounds, ModelEvent,
    ModelName, ProtocolLimits, ResponseId, ToolCallId, UsageScope,
};
use peritus_provider_core::{ProviderCoreError, SseFrame};
use serde_json::{Map, Value};

use super::interaction_fields::{append_arguments, correctness_critical, summary_text};
use super::state::NormalizeState;
use super::value::{
    cache, call_id, fragment, invalid, item_id, provider_event, required_str, required_u32,
    tool_name, usage,
};

enum ActiveStep {
    Message { item: ItemId, has_content: bool },
    Tool { item: ItemId, call: ToolCallId, arguments: Vec<u8> },
    Thought { item: ItemId, has_signature: bool },
}

pub(super) struct InteractionState {
    created: bool,
    response_id: Option<ResponseId>,
    active: Option<ActiveStep>,
    next_index: u32,
    structured: bool,
}

impl InteractionState {
    pub(super) const fn new(structured: bool) -> Self {
        Self { created: false, response_id: None, active: None, next_index: 0, structured }
    }

    pub(super) fn process(
        &mut self,
        owner: &mut NormalizeState,
        frame: &SseFrame,
        value: &Value,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        let kind = required_str(value, "/event_type")?;
        if frame.event().is_some_and(|event| event != kind) {
            return Err(invalid("Google Interactions SSE name and event_type disagree"));
        }
        match kind {
            "interaction.created" => self.created(owner, value, digest, frame.id()),
            "interaction.status_update" => {
                owner.emit(provider_event("interaction.status_update", value)?, digest, frame.id())
            }
            "step.start" => self.start(owner, value, digest, frame.id()),
            "step.delta" => self.delta(owner, value, digest, frame.id()),
            "step.stop" => self.stop(owner, value, digest, frame.id()),
            "interaction.completed" => self.completed(owner, value, digest, frame.id()),
            "error" => Self::error(owner, value, digest, frame.id()),
            "ping" => owner.emit(ModelEvent::Heartbeat, digest, frame.id()),
            unknown if correctness_critical(unknown) => {
                Err(invalid("Google emitted an unknown correctness-critical interaction event"))
            }
            unknown => owner.emit(provider_event(unknown, value)?, digest, frame.id()),
        }
    }

    fn created(
        &mut self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if self.created || self.active.is_some() {
            return Err(invalid("Google interaction was created more than once"));
        }
        let id = required_str(value, "/interaction/id")?;
        let response_id = ResponseId::new(id.to_owned())
            .map_err(|_| invalid("Google interaction identity is invalid"))?;
        let model = value
            .pointer("/interaction/model")
            .and_then(Value::as_str)
            .map(|model| ModelName::new(model.to_owned()))
            .transpose()
            .map_err(|_| invalid("Google interaction model identity is invalid"))?;
        self.created = true;
        self.response_id = Some(response_id.clone());
        owner.emit(
            ModelEvent::ResponseStarted { response_id: Some(response_id), model },
            digest,
            event_id,
        )?;
        owner.drain_metadata(digest, event_id)
    }

    fn start(
        &mut self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if !self.created || self.active.is_some() {
            return Err(invalid("Google interaction step start is out of order"));
        }
        let index = required_u32(value, "/index")?;
        if index != self.next_index {
            return Err(invalid("Google interaction step index is not monotonic"));
        }
        let kind = required_str(value, "/step/type")?;
        let item = item_id("step", index)?;
        match kind {
            "model_output" => {
                owner.emit(
                    ModelEvent::ItemStarted {
                        item_id: item.clone(),
                        index,
                        kind: if self.structured {
                            ItemKind::StructuredOutput
                        } else {
                            ItemKind::Message
                        },
                    },
                    digest,
                    event_id,
                )?;
                self.active = Some(ActiveStep::Message { item, has_content: false });
            }
            "function_call" => {
                let id = required_str(value, "/step/id")?;
                let name = required_str(value, "/step/name")?;
                let call = call_id(id)?;
                owner.emit(
                    ModelEvent::ItemStarted {
                        item_id: item.clone(),
                        index,
                        kind: ItemKind::ToolCall,
                    },
                    digest,
                    event_id,
                )?;
                owner.emit(
                    ModelEvent::ToolCallStarted {
                        item_id: item.clone(),
                        call_id: call.clone(),
                        name: tool_name(name)?,
                    },
                    digest,
                    event_id,
                )?;
                self.active = Some(ActiveStep::Tool { item, call, arguments: Vec::new() });
            }
            "thought" => {
                owner.emit(
                    ModelEvent::ItemStarted {
                        item_id: item.clone(),
                        index,
                        kind: ItemKind::Reasoning,
                    },
                    digest,
                    event_id,
                )?;
                self.active = Some(ActiveStep::Thought { item, has_signature: false });
            }
            _ => return Err(invalid("Google emitted an unsupported interaction step type")),
        }
        Ok(())
    }

    fn delta(
        &mut self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let index = required_u32(value, "/index")?;
        if index != self.next_index {
            return Err(invalid("Google interaction delta targeted another step"));
        }
        let kind = required_str(value, "/delta/type")?;
        match (&mut self.active, kind) {
            (Some(ActiveStep::Message { item, has_content }), "text") => {
                let text = required_str(value, "/delta/text")?;
                *has_content = true;
                owner.emit(
                    ModelEvent::TextDelta {
                        item_id: item.clone(),
                        fragment: fragment(text.as_bytes().to_vec())?,
                    },
                    digest,
                    event_id,
                )
            }
            (Some(ActiveStep::Tool { call, arguments, .. }), "arguments_delta") => {
                let bytes = required_str(value, "/delta/arguments")?.as_bytes().to_vec();
                append_arguments(arguments, &bytes)?;
                owner.emit(
                    ModelEvent::ToolArgumentDelta {
                        call_id: call.clone(),
                        fragment: fragment(bytes)?,
                    },
                    digest,
                    event_id,
                )
            }
            (Some(ActiveStep::Thought { item, .. }), "thought_summary") => {
                let text = summary_text(value)?;
                owner.emit(
                    ModelEvent::ReasoningSummaryDelta {
                        item_id: item.clone(),
                        fragment: fragment(text.as_bytes().to_vec())?,
                    },
                    digest,
                    event_id,
                )
            }
            (Some(ActiveStep::Thought { item, has_signature, .. }), "thought_signature") => {
                let signature = required_str(value, "/delta/signature")?;
                let mut replay = Map::new();
                replay.insert("type".to_owned(), Value::String("thought".to_owned()));
                replay.insert("signature".to_owned(), Value::String(signature.to_owned()));
                let bytes = serde_json::to_vec(&Value::Object(replay))
                    .map_err(|_| invalid("Google thought replay could not be serialized"))?;
                *has_signature = true;
                owner.emit(
                    ModelEvent::ReasoningReplayDelta {
                        item_id: item.clone(),
                        fragment: fragment(bytes)?,
                    },
                    digest,
                    event_id,
                )
            }
            (Some(_), "text_annotation_delta") => {
                owner.emit(provider_event("text_annotation_delta", value)?, digest, event_id)
            }
            _ => Err(invalid("Google interaction delta contradicts its active step")),
        }
    }

    fn stop(
        &mut self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let index = required_u32(value, "/index")?;
        if index != self.next_index {
            return Err(invalid("Google interaction stop targeted another step"));
        }
        let active = self.active.take().ok_or_else(|| invalid("Google stopped no active step"))?;
        let item = match active {
            ActiveStep::Message { item, has_content } => {
                if !has_content {
                    return Err(invalid("Google model-output step ended empty"));
                }
                item
            }
            ActiveStep::Tool { item, arguments, .. } => {
                let text = core::str::from_utf8(&arguments)
                    .map_err(|_| invalid("Google function arguments are not UTF-8"))?;
                let arguments =
                    CanonicalJson::parse(text, JsonBounds::value(ProtocolLimits::PRODUCTION))
                        .map_err(|_| {
                            invalid("Google function arguments are incomplete or malformed")
                        })?;
                if !arguments.is_object() {
                    return Err(invalid("Google function arguments are not an object"));
                }
                item
            }
            ActiveStep::Thought { item, has_signature } => {
                if !has_signature {
                    return Err(invalid("Google thought step ended without its signature"));
                }
                item
            }
        };
        owner.emit(ModelEvent::ItemCompleted(item), digest, event_id)?;
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or_else(|| invalid("Google step index overflowed"))?;
        Ok(())
    }

    fn completed(
        &self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if !self.created || self.active.is_some() {
            return Err(invalid("Google interaction completed with invalid step state"));
        }
        let interaction = value
            .get("interaction")
            .ok_or_else(|| invalid("Google completion omitted the interaction"))?;
        let id = required_str(value, "/interaction/id")?;
        if self.response_id.as_ref().is_some_and(|known| known.expose_for_wire() != id) {
            return Err(invalid("Google completion changed the interaction identity"));
        }
        if let Some(usage_value) = interaction.get("usage") {
            owner.emit(usage(usage_value, UsageScope::Final, true)?, digest, event_id)?;
            if let Some(cache) = cache(usage_value, true) {
                owner.emit(cache, digest, event_id)?;
            }
        }
        match required_str(value, "/interaction/status")? {
            "completed" => {
                owner.emit(ModelEvent::Finish(FinishReason::Stop), digest, event_id)?;
                owner.emit(ModelEvent::ResponseCompleted, digest, event_id)
            }
            "requires_action" => {
                owner.emit(ModelEvent::Finish(FinishReason::ToolCalls), digest, event_id)?;
                owner.emit(ModelEvent::ResponseCompleted, digest, event_id)
            }
            "incomplete" => {
                owner.emit(ModelEvent::Finish(FinishReason::Incomplete), digest, event_id)?;
                owner.emit(ModelEvent::ResponseCompleted, digest, event_id)
            }
            "cancelled" => owner.emit(ModelEvent::ResponseCancelled, digest, event_id),
            "failed" => Self::emit_failure(
                owner,
                FailureCategory::Provider,
                "google.interaction.failed",
                digest,
                event_id,
            ),
            _ => Err(invalid("Google completion used an unknown interaction status")),
        }
    }

    fn error(
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let code = required_str(value, "/error/code")?;
        let category = match code {
            "authentication" => FailureCategory::Authentication,
            "permission_denied" => FailureCategory::Permission,
            "rate_limit_exceeded" => FailureCategory::RateLimited,
            "quota_exceeded" => FailureCategory::QuotaExhausted,
            "cancelled" => FailureCategory::Cancellation,
            "api_error" | "service_unavailable" | "deadline_exceeded" => {
                FailureCategory::TransientProvider
            }
            _ => FailureCategory::Provider,
        };
        Self::emit_failure(owner, category, "google.interaction.error", digest, event_id)
    }

    fn emit_failure(
        owner: &mut NormalizeState,
        category: FailureCategory,
        code: &'static str,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let failure = crate::error::stream_failure(
            owner.provider.clone(),
            category,
            owner.has_observed_semantics(),
            code,
        )?;
        owner.emit(ModelEvent::ResponseFailed(failure), digest, event_id)
    }
}
