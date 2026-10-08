//! Completed-item validation and fail-closed terminal classification.

use super::{ItemAssembly, ReducedItem, ReducerTransition, ResponseReducer};
use crate::{
    BoundedText, CanonicalJson, CompletedToolCall, FailureCategory, FinishReason, ItemId, ItemKind,
    JsonBounds, ModelFailure, OutcomeCertainty, ProtocolError, ProtocolErrorKind, ProtocolLimits,
    RedactedDiagnostic, Retryability, TerminalOutcome, TransportPhase,
};

impl ResponseReducer {
    pub(super) fn reduce_item(
        &self,
        item_id: ItemId,
        item: &ItemAssembly,
    ) -> Result<ReducedItem, ProtocolError> {
        match item.kind {
            ItemKind::Message => Ok(ReducedItem::Text {
                item_id,
                index: item.index,
                text: complete_text(item.content.clone(), self.limits)?,
            }),
            ItemKind::StructuredOutput => {
                let text = std::str::from_utf8(&item.content)
                    .map_err(|_| invalid("structured output ended with incomplete UTF-8"))?;
                let value = CanonicalJson::parse(text, JsonBounds::value(self.limits))?;
                Ok(ReducedItem::Structured { item_id, index: item.index, value })
            }
            ItemKind::ToolCall => {
                let call = item
                    .call
                    .as_ref()
                    .ok_or_else(|| invalid("tool item ended without a call start"))?;
                let text = std::str::from_utf8(&call.arguments)
                    .map_err(|_| invalid("tool arguments ended with incomplete UTF-8"))?;
                let arguments = CanonicalJson::parse(text, JsonBounds::value(self.limits))?;
                let call = CompletedToolCall::new(call.id.clone(), call.name.clone(), arguments)?;
                Ok(ReducedItem::ToolCall { item_id, index: item.index, call })
            }
            ItemKind::Reasoning => {
                let summary = if item.content.is_empty() {
                    None
                } else {
                    Some(complete_text(item.content.clone(), self.limits)?)
                };
                if summary.is_none() && item.replay.is_empty() {
                    return Err(invalid("reasoning item ended without summary or replay state"));
                }
                if self.provider.as_str() == "google" && !item.replay.is_empty() {
                    validate_google_replay(&item.replay, self.limits)?;
                }
                Ok(ReducedItem::Reasoning {
                    item_id,
                    index: item.index,
                    summary,
                    replay: item.replay.clone(),
                })
            }
            ItemKind::Refusal => Ok(ReducedItem::Refusal {
                item_id,
                index: item.index,
                text: complete_text(item.content.clone(), self.limits)?,
            }),
            ItemKind::ProviderNative => {
                if item.content.is_empty() {
                    return Err(invalid("provider-native item is empty"));
                }
                Ok(ReducedItem::ProviderNative {
                    item_id,
                    index: item.index,
                    bytes: item.content.clone(),
                })
            }
        }
    }

    pub(super) fn complete_response(&mut self) -> Result<ReducerTransition, ProtocolError> {
        if self.items.values().any(|item| !item.complete) {
            return self.reject("response completed with open items or calls");
        }
        let Some(reason) = self.finish.clone() else {
            return self.reject("response completed without a finish reason");
        };
        let terminal = match reason {
            FinishReason::Stop => TerminalOutcome::Succeeded { reason },
            FinishReason::ToolCalls | FinishReason::Pause => {
                TerminalOutcome::RequiresAction { reason }
            }
            FinishReason::Refusal | FinishReason::Safety => TerminalOutcome::Refused { reason },
            FinishReason::Cancelled => TerminalOutcome::Cancelled,
            FinishReason::Length
            | FinishReason::ContextLimit
            | FinishReason::Incomplete
            | FinishReason::Provider(_) => TerminalOutcome::Incomplete { reason },
        };
        Ok(self.set_terminal(terminal))
    }

    pub(super) fn set_terminal(&mut self, terminal: TerminalOutcome) -> ReducerTransition {
        self.terminal = Some(terminal.clone());
        ReducerTransition::Terminal(terminal)
    }

    pub(super) fn account_output(&mut self, additional: usize) -> Result<(), ProtocolError> {
        let Some(next) = self.output_bytes.checked_add(additional) else {
            return self.reject_unit("assembled output byte count overflowed");
        };
        if next > self.limits.max_output_bytes() {
            return self.reject_unit("assembled output exceeds its byte bound");
        }
        self.output_bytes = next;
        Ok(())
    }

    pub(super) fn reject<T>(&mut self, detail: &'static str) -> Result<T, ProtocolError> {
        self.mark_malformed();
        Err(invalid(detail))
    }

    pub(super) fn mark_malformed(&mut self) {
        if self.terminal.is_none() {
            self.terminal = Some(TerminalOutcome::Failed(
                self.failure(FailureCategory::MalformedPayload, "malformed_stream"),
            ));
        }
    }

    pub(super) fn reject_unit(&mut self, detail: &'static str) -> Result<(), ProtocolError> {
        self.reject(detail)
    }

    pub(super) fn failure(&self, category: FailureCategory, code: &'static str) -> ModelFailure {
        let diagnostic = RedactedDiagnostic::new(
            code.to_owned(),
            None,
            u64::try_from(self.output_bytes).ok(),
            None,
        )
        .expect("static reducer diagnostic is valid");
        ModelFailure::new(
            self.provider.clone(),
            category,
            if self.started { TransportPhase::StreamObserved } else { TransportPhase::ReadingBody },
            if self.output_bytes == 0 {
                OutcomeCertainty::MaybeAccepted
            } else {
                OutcomeCertainty::AcceptedPartial
            },
            Retryability::Never,
            None,
            self.response_id.clone(),
            None,
            diagnostic,
        )
    }
}

fn complete_text(bytes: Vec<u8>, limits: ProtocolLimits) -> Result<BoundedText, ProtocolError> {
    let text = String::from_utf8(bytes).map_err(|_| invalid("text ended with incomplete UTF-8"))?;
    BoundedText::new(text, limits)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum GoogleReplayShape {
    Interaction,
    Generate,
}

fn validate_google_replay(bytes: &[u8], limits: ProtocolLimits) -> Result<(), ProtocolError> {
    let mut stream = serde_json::Deserializer::from_slice(bytes).into_iter::<serde_json::Value>();
    let mut start = 0;
    let mut shape = None;
    let mut objects = 0_usize;
    while let Some(value) = stream.next() {
        let value = value.map_err(|_| {
            invalid("Google reasoning replay ended without complete signature boundaries")
        })?;
        let end = stream.byte_offset();
        let text = core::str::from_utf8(&bytes[start..end])
            .map_err(|_| invalid("Google reasoning replay is not UTF-8 JSON"))?;
        let canonical = CanonicalJson::parse(text, JsonBounds::extension(limits))?;
        if !canonical.is_object() {
            return Err(invalid("Google reasoning replay signature is not an object"));
        }
        let object = value
            .as_object()
            .ok_or_else(|| invalid("Google reasoning replay signature is not an object"))?;
        let current = if object.get("type").and_then(serde_json::Value::as_str) == Some("thought")
            && object.get("signature").and_then(serde_json::Value::as_str).is_some()
            && object.keys().all(|key| matches!(key.as_str(), "type" | "signature"))
        {
            GoogleReplayShape::Interaction
        } else if object
            .get("thoughtSignature")
            .and_then(serde_json::Value::as_str)
            .is_some()
            && object.keys().all(|key| key == "thoughtSignature")
        {
            GoogleReplayShape::Generate
        } else {
            return Err(invalid("Google reasoning replay has an unsupported signature shape"));
        };
        if shape.is_some_and(|known| known != current) {
            return Err(invalid("Google reasoning replay mixes signature dialects"));
        }
        shape = Some(current);
        objects = objects
            .checked_add(1)
            .ok_or_else(|| invalid("Google reasoning replay signature count overflowed"))?;
        start = end;
    }
    if objects == 0 {
        return Err(invalid("Google reasoning replay contains no complete signature object"));
    }
    Ok(())
}

fn invalid(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEvent, "stream", detail)
}
