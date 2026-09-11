//! Hosted accounting, provider errors, and exact reasoning replay completion.
use super::{ChatDecoder, FrameEvents, integer};
use crate::error;
use peritus_model_protocol::{ItemId, ItemKind, ModelEvent, StreamFragment};
use peritus_provider_core::{ProviderCoreError, SseFrame};
use serde_json::Value;

pub(super) struct CompletedChoice {
    pub(super) wire_reason: String,
    pub(super) accounting_seen: bool,
}

impl ChatDecoder {
    pub(super) fn validate_tool_choice(&self) -> Result<(), ProviderCoreError> {
        use peritus_model_protocol::ToolChoice;
        let valid = match &self.tool_choice {
            ToolChoice::Specific(name) => {
                self.tools.len() == 1 && self.tools.values().all(|tool| &tool.name == name)
            }
            ToolChoice::Required => !self.tools.is_empty(),
            ToolChoice::None => self.tools.is_empty(),
            ToolChoice::Auto => true,
        };
        if !valid {
            return Err(error::malformed(
                "hosted provider did not return the required tool choice",
            ));
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
        if finish.accounting_seen
            || integer(choice, "index")? != 0
            || choice.get("finish_reason").and_then(Value::as_str)
                != Some(finish.wire_reason.as_str())
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
        &self,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ProviderCoreError> {
        if let Some(service) = self.service
            && !self.reasoning.is_empty()
        {
            let response = self
                .response_id
                .as_ref()
                .ok_or_else(|| error::malformed("reasoning has no response identity"))?;
            let item_id = ItemId::new(format!("{}-reasoning", response.expose_for_wire()))
                .map_err(|_| error::malformed("reasoning item identity is invalid"))?;
            let bytes = serde_json::to_vec(
                &serde_json::json!({"service":service.name(),"fields":self.reasoning}),
            )
            .map_err(|_| error::malformed("reasoning replay serialization failed"))?;
            events.push(ModelEvent::ItemStarted {
                item_id: item_id.clone(),
                index: 2,
                kind: ItemKind::Reasoning,
            });
            // Emit bounded fragments while retaining one exact replay object in the reducer.
            for bytes in bytes.chunks(self.limits.max_event_bytes().min(16_384)) {
                events.push(ModelEvent::ReasoningReplayDelta {
                    item_id: item_id.clone(),
                    fragment: StreamFragment::new(bytes.to_vec(), self.limits)
                        .map_err(|_| error::limit("reasoning replay fragment exceeds bounds"))?,
                });
            }
            events.push(ModelEvent::ItemCompleted(item_id));
        }
        Ok(())
    }
}
