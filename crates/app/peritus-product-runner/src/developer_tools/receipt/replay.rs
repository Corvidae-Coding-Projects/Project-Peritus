//! Read-only completed-receipt lookup before mutation-specific checkpoint preflight.

use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::CompletedToolCall;

use super::{EffectReceiptLedger, ReceiptDecision, ReceiptState, ambiguous, request_digest, tool};

impl EffectReceiptLedger {
    pub(in crate::developer_tools) fn replay(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<Option<ReceiptDecision>, DeveloperLoopError> {
        self.load()?;
        let ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| tool("effect receipt ordinal overflowed"))?;
        let digest = request_digest(call);
        let Some(existing) = self.entries.get(&ordinal) else { return Ok(None) };
        if existing.tool != call.name().as_str() || existing.request_sha256 != digest {
            self.next_ordinal = ordinal;
            return Ok(Some(ReceiptDecision::Refuse {
                detail: format!(
                    "effect receipt conflict at {} effect {}: the recovered request differs from the durably started request",
                    self.scope, ordinal
                ),
                ambiguous: false,
            }));
        }
        let decision = match existing.state {
            ReceiptState::Completed => ReceiptDecision::Replay {
                value: existing
                    .output
                    .clone()
                    .ok_or_else(|| tool("completed receipt lost its result"))?,
                is_error: existing
                    .is_error
                    .ok_or_else(|| tool("completed receipt lost its result status"))?,
            },
            ReceiptState::Ambiguous => ReceiptDecision::Refuse {
                detail: ambiguous(&self.scope, ordinal, &existing.call_id),
                ambiguous: true,
            },
            ReceiptState::Started => return Ok(None),
        };
        self.next_ordinal = ordinal;
        Ok(Some(decision))
    }
}
