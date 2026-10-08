//! Hosted accounting, provider errors, and exact reasoning replay completion.
use std::collections::VecDeque;

use super::{ChatDecoder, FrameEvents, integer};
use crate::error;
use peritus_model_protocol::{ItemId, ItemKind, ModelEvent, ProtocolLimits, StreamFragment};
use peritus_provider_core::{ProviderCoreError, SseFrame, hosted::HostedService};
use serde_json::Value;

pub(super) const REQUIRED_TOOL_CHOICE_MISSING: &str =
    "hosted provider did not return the required tool choice";

pub(super) struct CompletedChoice {
    pub(super) wire_reason: String,
    pub(super) accounting_seen: bool,
}

pub(crate) struct ReasoningCompletion {
    item_id: ItemId,
    replay: crate::hosted_reasoning::ReplayBytes,
    limits: ProtocolLimits,
    item_completed: bool,
    trailing: VecDeque<ModelEvent>,
}

impl ReasoningCompletion {
    pub(crate) const fn replaying(&self) -> bool {
        !self.item_completed
    }

    pub(crate) fn next_event(&mut self) -> Result<Option<ModelEvent>, ProviderCoreError> {
        if !self.item_completed {
            let fragment_bytes = self.limits.max_event_bytes().min(16_384);
            if let Some(bytes) = self.replay.next_chunk(fragment_bytes)? {
                let fragment = StreamFragment::new(bytes, self.limits)
                    .map_err(|_| error::limit("reasoning replay fragment exceeds bounds"))?;
                return Ok(Some(ModelEvent::ReasoningReplayDelta {
                    item_id: self.item_id.clone(),
                    fragment,
                }));
            }
            self.item_completed = true;
            return Ok(Some(ModelEvent::ItemCompleted(self.item_id.clone())));
        }
        Ok(self.trailing.pop_front())
    }
}

impl ChatDecoder {
    pub(super) fn decode_choices(
        &mut self,
        value: &Value,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| error::malformed("Chat-compatible chunk omitted choices"))?;
        let flattened =
            matches!(self.service, Some(HostedService::OpenCodeZen | HostedService::OpenCodeGo));
        if choices.len() > 1 && !flattened {
            return Err(error::malformed("Chat-compatible multiple choices are not mapped"));
        }
        for choice in choices {
            let accounting =
                self.finish.is_some() && value.get("usage").is_some_and(|value| !value.is_null());
            if accounting {
                self.accounting(choice)?;
            } else {
                self.choice(choice, events)?;
            }
        }
        Ok(())
    }

    pub(super) fn validate_tool_choice(&self) -> Result<(), ProviderCoreError> {
        use peritus_model_protocol::ToolChoice;
        let valid = match &self.tool_choice {
            ToolChoice::Specific(name) => {
                !self.tools.is_empty() && self.tools.values().all(|tool| &tool.name == name)
            }
            ToolChoice::Required => !self.tools.is_empty(),
            ToolChoice::None => self.tools.is_empty(),
            ToolChoice::Auto => true,
        };
        if !valid {
            return Err(error::malformed(REQUIRED_TOOL_CHOICE_MISSING));
        }
        Ok(())
    }

    pub(super) fn provider_failure(
        &self,
        value: &Value,
        frame: &SseFrame,
    ) -> Result<FrameEvents, ProviderCoreError> {
        use peritus_model_protocol::{
            FailureCategory, OutcomeCertainty, Retryability, TransportPhase,
        };
        let status =
            value.get("code").and_then(Value::as_u64).and_then(|value| u16::try_from(value).ok());
        let category = match status {
            Some(400) => FailureCategory::InvalidRequest,
            Some(401) => FailureCategory::Authentication,
            Some(402) => FailureCategory::QuotaExhausted,
            Some(403) => FailureCategory::Permission,
            Some(404) => FailureCategory::NotFound,
            Some(429) => FailureCategory::RateLimited,
            _ => FailureCategory::TransientProvider,
        };
        let failure = error::failure(
            &self.provider,
            category,
            TransportPhase::StreamObserved,
            OutcomeCertainty::MaybeAccepted,
            Retryability::Never,
            status,
            self.response_id.clone(),
            None,
            "hosted.openrouter.stream_error",
        )?;
        Ok(FrameEvents {
            provider_sequence: None,
            provider_event_id: None,
            digest: peritus_codec::sha256(frame.data().as_bytes()),
            events: vec![ModelEvent::ResponseFailed(failure)],
            continuation: None,
        })
    }

    pub(super) fn accounting(&mut self, choice: &Value) -> Result<(), ProviderCoreError> {
        super::fields::validate_choice(choice, self.service)?;
        let finish =
            self.finish.as_mut().ok_or_else(|| error::malformed("accounting preceded finish"))?;
        let delta = choice
            .get("delta")
            .and_then(Value::as_object)
            .ok_or_else(|| error::malformed("final accounting chunk omitted delta"))?;
        let finish_reason_matches = match choice.get("finish_reason") {
            None | Some(Value::Null) => true,
            Some(Value::String(reason)) => reason == &finish.wire_reason,
            Some(_) => false,
        };
        if finish.accounting_seen
            || integer(choice, "index")? != 0
            || !finish_reason_matches
            || delta.iter().any(|(name, value)| {
                name != "content" || !(value.is_null() || value.as_str() == Some(""))
            })
        {
            return Err(error::malformed(
                "final accounting chunk contained output or changed its finish",
            ));
        }
        finish.accounting_seen = true;
        Ok(())
    }

    pub(super) fn finish_reasoning(
        &mut self,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        if let Some(service) = self.service
            && !self.reasoning.is_empty()
        {
            let response = self
                .response_id
                .as_ref()
                .ok_or_else(|| error::malformed("reasoning has no response identity"))?;
            let item_id =
                super::super::identity::item_id(response.expose_for_wire(), "-reasoning")?;
            let reasoning = core::mem::take(&mut self.reasoning);
            let replay = reasoning.replay(service, self.limits.max_output_bytes())?;
            events.push(ModelEvent::ItemStarted {
                item_id: item_id.clone(),
                index: 2,
                kind: ItemKind::Reasoning,
            });
            self.reasoning_split = Some(events.len());
            self.reasoning_completion = Some(ReasoningCompletion {
                item_id,
                replay,
                limits: self.limits,
                item_completed: false,
                trailing: VecDeque::new(),
            });
        }
        Ok(())
    }

    pub(super) fn take_reasoning_completion(
        &mut self,
        events: &mut Vec<ModelEvent>,
    ) -> Option<ReasoningCompletion> {
        let split = self.reasoning_split.take()?;
        let mut completion = self.reasoning_completion.take()?;
        completion.trailing.extend(events.drain(split..));
        Some(completion)
    }
}
