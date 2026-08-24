//! Stable-v1 Generate Content streamed response and candidate normalization.

use peritus_model_protocol::{
    BoundedText, FailureCategory, FinishReason, ItemId, ItemKind, ModelEvent, ModelName,
    ProtocolLimits, ResponseId, UsageScope,
};
use peritus_provider_core::{ProviderCoreError, SseFrame};
use serde_json::{Map, Value};

use super::state::NormalizeState;
use super::value::{cache, call_id, fragment, invalid, item_id, provider_event, tool_name, usage};

pub(super) struct GenerateState {
    started: bool,
    response_id: Option<ResponseId>,
    active: Option<(ItemId, ItemKind)>,
    next_item: u32,
    structured: bool,
    saw_tool: bool,
}

impl GenerateState {
    pub(super) const fn new(structured: bool) -> Self {
        Self {
            started: false,
            response_id: None,
            active: None,
            next_item: 0,
            structured,
            saw_tool: false,
        }
    }

    pub(super) fn process(
        &mut self,
        owner: &mut NormalizeState,
        frame: &SseFrame,
        value: &Value,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        if frame.event().is_some_and(|event| event != "message") {
            return Err(invalid("Generate Content emitted an unknown SSE event name"));
        }
        if value.get("event_type").is_some() {
            return Err(invalid("Generate Content emitted an Interaction-shaped stream event"));
        }
        if value.get("error").is_some() {
            return Self::error(owner, value, digest, frame.id());
        }
        if !value.is_object() {
            return Err(invalid("Generate Content stream item is not an object"));
        }
        self.ensure_started(owner, value, digest, frame.id())?;
        self.identity(owner, value, digest, frame.id())?;
        Self::ancillary(owner, value, digest, frame.id())?;
        let blocked =
            value.pointer("/promptFeedback/blockReason").and_then(Value::as_str).is_some();
        if let Some(feedback) = value.get("promptFeedback") {
            owner.emit(provider_event("prompt_feedback", feedback)?, digest, frame.id())?;
        }
        let candidates = value.get("candidates").and_then(Value::as_array);
        let mut finish_reason = None;
        if let Some(candidates) = candidates {
            if candidates.len() > 1 {
                return Err(invalid("Generate Content returned multiple unrequested candidates"));
            }
            if let Some(candidate) = candidates.first() {
                if candidate.get("index").and_then(Value::as_u64).unwrap_or(0) != 0 {
                    return Err(invalid("Generate Content returned a nonzero candidate index"));
                }
                self.parts(owner, candidate, digest, frame.id())?;
                finish_reason = candidate.get("finishReason").and_then(Value::as_str);
                if let Some(ratings) = candidate.get("safetyRatings") {
                    owner.emit(
                        provider_event("candidate.safety_ratings", ratings)?,
                        digest,
                        frame.id(),
                    )?;
                }
            }
        }
        if let Some(usage_value) = value.get("usageMetadata") {
            let scope = if finish_reason.is_some() || blocked {
                UsageScope::Final
            } else {
                UsageScope::Cumulative
            };
            owner.emit(usage(usage_value, scope, false)?, digest, frame.id())?;
            if let Some(cache) = cache(usage_value, false) {
                owner.emit(cache, digest, frame.id())?;
            }
        }
        if blocked {
            self.close_active(owner, digest, frame.id())?;
            owner.emit(ModelEvent::Finish(FinishReason::Safety), digest, frame.id())?;
            owner.emit(ModelEvent::ResponseCompleted, digest, frame.id())?;
        } else if let Some(reason) = finish_reason {
            self.finish(owner, reason, digest, frame.id())?;
        }
        Ok(())
    }

    fn ancillary(
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let mut unknown = Map::new();
        for (key, field) in value.as_object().into_iter().flatten() {
            if !matches!(
                key.as_str(),
                "candidates" | "promptFeedback" | "usageMetadata" | "modelVersion" | "responseId"
            ) {
                unknown.insert(key.clone(), field.clone());
            }
        }
        if !unknown.is_empty() {
            owner.emit(
                provider_event("generate.ancillary", &Value::Object(unknown))?,
                digest,
                event_id,
            )?;
        }
        Ok(())
    }

    fn error(
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let code = value
            .pointer("/error/status")
            .or_else(|| value.pointer("/error/code"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let category = match code {
            "UNAUTHENTICATED" => FailureCategory::Authentication,
            "PERMISSION_DENIED" => FailureCategory::Permission,
            "RESOURCE_EXHAUSTED" | "RATE_LIMIT_EXCEEDED" => FailureCategory::RateLimited,
            "CANCELLED" => FailureCategory::Cancellation,
            "UNAVAILABLE" | "INTERNAL" | "DEADLINE_EXCEEDED" => FailureCategory::TransientProvider,
            _ => FailureCategory::Provider,
        };
        let failure = crate::error::stream_failure(
            owner.provider.clone(),
            category,
            owner.has_observed_semantics(),
            "google.generate.error",
        )?;
        owner.emit(ModelEvent::ResponseFailed(failure), digest, event_id)
    }

    fn ensure_started(
        &mut self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if self.started {
            return Ok(());
        }
        let response_id = value
            .get("responseId")
            .and_then(Value::as_str)
            .map(|id| ResponseId::new(id.to_owned()))
            .transpose()
            .map_err(|_| invalid("Generate Content response identity is invalid"))?;
        let model = value
            .get("modelVersion")
            .and_then(Value::as_str)
            .map(|model| ModelName::new(model.to_owned()))
            .transpose()
            .map_err(|_| invalid("Generate Content model identity is invalid"))?;
        self.response_id.clone_from(&response_id);
        self.started = true;
        owner.emit(ModelEvent::ResponseStarted { response_id, model }, digest, event_id)?;
        owner.drain_metadata(digest, event_id)
    }

    fn identity(
        &mut self,
        owner: &mut NormalizeState,
        value: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let Some(id) = value.get("responseId").and_then(Value::as_str) else {
            return Ok(());
        };
        if self.response_id.as_ref().is_some_and(|known| known.expose_for_wire() != id) {
            return Err(invalid("Generate Content changed its response identity"));
        }
        if self.response_id.is_none() {
            let id = ResponseId::new(id.to_owned())
                .map_err(|_| invalid("Generate Content response identity is invalid"))?;
            self.response_id = Some(id.clone());
            owner.emit(ModelEvent::ResponseIdentity(id), digest, event_id)?;
        }
        Ok(())
    }

    fn parts(
        &mut self,
        owner: &mut NormalizeState,
        candidate: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let Some(parts) = candidate.pointer("/content/parts").and_then(Value::as_array) else {
            return Ok(());
        };
        for part in parts {
            let signature = part.get("thoughtSignature").and_then(Value::as_str);
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                let thought = part.get("thought").and_then(Value::as_bool).unwrap_or(false);
                let kind = if thought {
                    ItemKind::Reasoning
                } else if self.structured {
                    ItemKind::StructuredOutput
                } else {
                    ItemKind::Message
                };
                let item = self.ensure_item(owner, kind, digest, event_id)?;
                let event = if thought {
                    ModelEvent::ReasoningSummaryDelta {
                        item_id: item,
                        fragment: fragment(text.as_bytes().to_vec())?,
                    }
                } else {
                    ModelEvent::TextDelta {
                        item_id: item,
                        fragment: fragment(text.as_bytes().to_vec())?,
                    }
                };
                owner.emit(event, digest, event_id)?;
            }
            if let Some(signature) = signature {
                let item = self.ensure_item(owner, ItemKind::Reasoning, digest, event_id)?;
                let mut replay = Map::new();
                replay.insert("thoughtSignature".to_owned(), Value::String(signature.to_owned()));
                let bytes = serde_json::to_vec(&Value::Object(replay))
                    .map_err(|_| invalid("Google thought signature could not be serialized"))?;
                owner.emit(
                    ModelEvent::ReasoningReplayDelta { item_id: item, fragment: fragment(bytes)? },
                    digest,
                    event_id,
                )?;
            }
            if let Some(call) = part.get("functionCall") {
                self.close_active(owner, digest, event_id)?;
                self.function_call(owner, call, digest, event_id)?;
            } else if part.get("text").is_none() && signature.is_none() {
                return Err(invalid(
                    "Generate Content emitted an unknown correctness-critical part",
                ));
            }
        }
        Ok(())
    }

    fn ensure_item(
        &mut self,
        owner: &mut NormalizeState,
        kind: ItemKind,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<ItemId, ProviderCoreError> {
        if let Some((item, active_kind)) = &self.active
            && *active_kind == kind
        {
            return Ok(item.clone());
        }
        self.close_active(owner, digest, event_id)?;
        let item = item_id("candidate", self.next_item)?;
        owner.emit(
            ModelEvent::ItemStarted { item_id: item.clone(), index: self.next_item, kind },
            digest,
            event_id,
        )?;
        self.active = Some((item.clone(), kind));
        self.next_item = self
            .next_item
            .checked_add(1)
            .ok_or_else(|| invalid("Generate Content item index overflowed"))?;
        Ok(item)
    }

    fn function_call(
        &mut self,
        owner: &mut NormalizeState,
        call: &Value,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let name = call
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Generate Content function name is missing"))?;
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .map_or_else(|| format!("google-call-{}", self.next_item), str::to_owned);
        let arguments = call
            .get("args")
            .ok_or_else(|| invalid("Generate Content function arguments are missing"))?;
        if !arguments.is_object() {
            return Err(invalid("Generate Content function arguments are not an object"));
        }
        let item = self.ensure_item(owner, ItemKind::ToolCall, digest, event_id)?;
        let call_id = call_id(&id)?;
        owner.emit(
            ModelEvent::ToolCallStarted {
                item_id: item,
                call_id: call_id.clone(),
                name: tool_name(name)?,
            },
            digest,
            event_id,
        )?;
        let bytes = serde_json::to_vec(arguments)
            .map_err(|_| invalid("Generate Content function arguments could not be serialized"))?;
        owner.emit(
            ModelEvent::ToolArgumentDelta { call_id, fragment: fragment(bytes)? },
            digest,
            event_id,
        )?;
        self.close_active(owner, digest, event_id)?;
        self.saw_tool = true;
        Ok(())
    }

    fn close_active(
        &mut self,
        owner: &mut NormalizeState,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if let Some((item, _kind)) = self.active.take() {
            owner.emit(ModelEvent::ItemCompleted(item), digest, event_id)?;
        }
        Ok(())
    }

    fn finish(
        &mut self,
        owner: &mut NormalizeState,
        reason: &str,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        self.close_active(owner, digest, event_id)?;
        let reason = match reason {
            "STOP" if self.saw_tool => FinishReason::ToolCalls,
            "STOP" => FinishReason::Stop,
            "MAX_TOKENS" => FinishReason::Length,
            "SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY" => {
                FinishReason::Safety
            }
            "MALFORMED_FUNCTION_CALL" | "UNEXPECTED_TOOL_CALL" => FinishReason::Incomplete,
            raw => FinishReason::Provider(
                BoundedText::new(raw.to_owned(), ProtocolLimits::PRODUCTION)
                    .map_err(|_| invalid("Generate Content finish reason is invalid"))?,
            ),
        };
        owner.emit(ModelEvent::Finish(reason), digest, event_id)?;
        owner.emit(ModelEvent::ResponseCompleted, digest, event_id)
    }
}
