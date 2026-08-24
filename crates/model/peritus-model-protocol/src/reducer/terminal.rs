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

fn invalid(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEvent, "stream", detail)
}
