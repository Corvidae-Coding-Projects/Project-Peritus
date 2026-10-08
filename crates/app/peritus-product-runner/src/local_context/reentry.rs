//! Exact invocation reopening for a durably scheduled definitely-unaccepted request.

use peritus_agent::{
    DeveloperContextResume, DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopRequest,
};
use peritus_model_protocol::{Message, ProtocolLimits, decode_messages, encode_messages};

use super::{
    error,
    memory::LocalMemory,
    record::{
        MemoryRecord, SEGMENT_CONTINUATION_SCHEMA_VERSION,
    },
};

impl LocalMemory {
    pub(super) fn invocation_scope(
        &self,
        request_prefix: &str,
    ) -> Result<u64, DeveloperLoopError> {
        if let Some(invocation) = self.invocation_for_prefix(request_prefix) {
            if let Some(pending) = crate::trace::pending_retry(&self.trace_path)? {
                let segment = self.segment_sequence(request_prefix)?;
                if expected_request_digest(
                    request_prefix,
                    segment,
                    pending.turn(),
                    pending.attempt(),
                )
                    != pending.request_id_sha256()
                {
                    return Err(error("retry trace does not bind the current invocation"));
                }
                return Ok(invocation);
            }
            if self.segment_continuation.is_some() {
                self.segment_sequence(request_prefix)?;
                return Ok(invocation);
            }
        }
        self.transcript
            .invocation
            .checked_add(1)
            .ok_or_else(|| error("invocation sequence overflow"))
    }

    pub(super) fn pending_reentry_prefix(
        &self,
        expected_prefix: &str,
    ) -> Result<Option<String>, DeveloperLoopError> {
        Ok(self
            .pending_developer_reentry(expected_prefix)?
            .map(|(request_prefix, _)| request_prefix))
    }

    pub(super) fn pending_developer_reentry(
        &self,
        expected_prefix: &str,
    ) -> Result<Option<(String, u32)>, DeveloperLoopError> {
        let pending = crate::trace::pending_retry(&self.trace_path)?;
        if pending.is_none() && self.segment_continuation.is_none() {
            return Ok(None);
        }
        let request_prefix = self.transcript.request_prefix.clone();
        let invocation = self
            .invocation_for_prefix(&request_prefix)
            .ok_or_else(|| error("pending reentry has no exact indexed invocation"))?;
        let segment = self.segment_sequence(&request_prefix)?;
        if invocation == 0 || self.last_view.is_empty() {
            return Err(error("pending reentry does not bind the durable invocation"));
        }
        if let Some(pending) = pending.as_ref()
            && expected_request_digest(
                &request_prefix,
                segment,
                pending.turn(),
                pending.attempt(),
            ) != pending.request_id_sha256()
        {
            return Err(error("pending retry does not bind the durable invocation"));
        }
        if !super::grounding::valid_invocation(
            &request_prefix,
            expected_prefix,
        ) {
            if let Some(pending) = pending {
                let physical = DeveloperLoopLimits::request_prefix_for_segment(
                    &request_prefix,
                    segment,
                );
                crate::trace::supersede_pending_retry(
                    &self.trace_path,
                    &physical,
                    pending.turn(),
                    pending.attempt(),
                )?;
            }
            return Ok(None);
        }
        Ok(Some((request_prefix, segment)))
    }

    pub(super) fn resume(
        &self,
        request: &DeveloperLoopRequest,
        initial_messages: &[Message],
    ) -> Result<Option<DeveloperContextResume>, DeveloperLoopError> {
        let pending = crate::trace::pending_retry(&self.trace_path)?;
        let segment = self.segment_continuation.as_ref();
        if pending.is_none() && segment.is_none() {
            return Ok(None);
        }
        let invocation = self.invocation_for_prefix(&request.request_prefix);
        let segment_sequence = self.segment_sequence(&request.request_prefix)?;
        if invocation.is_none() {
            return Err(error("pending reentry request identity changed"));
        }
        if let Some(pending) = pending.as_ref()
            && expected_request_digest(
                &request.request_prefix,
                segment_sequence,
                pending.turn(),
                pending.attempt(),
            ) != pending.request_id_sha256()
        {
            return Err(error("pending retry request identity changed"));
        }
        if self.transcript.pending.iter().any(|pending| pending.handle.is_none()) {
            return Err(DeveloperLoopError::RecoveryRequired(
                "an exact proposed tool exchange remains unresolved in the retrying invocation"
                    .to_owned(),
            ));
        }
        let durable_inputs = self.current_inputs()?;
        if segment.is_none()
            && encode_messages(&durable_inputs, ProtocolLimits::PRODUCTION)?
                != encode_messages(initial_messages, ProtocolLimits::PRODUCTION)?
        {
            return Err(DeveloperLoopError::RecoveryRequired(
                "current invocation inputs changed before exact request reentry".to_owned(),
            ));
        }
        let (turn, retries) = pending.as_ref().map_or((1, 0), |pending| {
            (pending.turn(), pending.attempt().saturating_sub(1))
        });
        let inputs = if segment.is_some() {
            durable_inputs.as_slice()
        } else {
            initial_messages
        };
        let resume = if segment.is_some() {
            DeveloperContextResume::new_segment(
                turn,
                inputs,
                self.last_view.clone(),
                0,
                retries,
            )?
        } else {
            let tool_calls = u32::try_from(
                self.sources
                    .iter()
                    .filter(|source| {
                        Some(source.invocation) == invocation
                            && source.kind == super::record::ArchiveKind::ToolOutput
                    })
                    .count(),
            )
            .map_err(|_| error("durable invocation tool count overflow"))?;
            DeveloperContextResume::new(
                turn,
                inputs,
                self.last_view.clone(),
                tool_calls,
                retries,
            )?
        };
        Ok(Some(resume))
    }

    pub(super) fn segment_sequence(
        &self,
        request_prefix: &str,
    ) -> Result<u32, DeveloperLoopError> {
        let Some(segment) = self.segment_continuation.as_ref() else {
            return Ok(0);
        };
        if segment.schema_version != SEGMENT_CONTINUATION_SCHEMA_VERSION
            || segment.invocation != self.transcript.invocation
            || segment.request_prefix != request_prefix
            || segment.request_prefix != self.transcript.request_prefix
            || segment.segment_sequence == 0
            || segment.protocol_limits_sha256 != protocol_limits_sha256()?
        {
            return Err(error("durable segment continuation binding mismatch"));
        }
        Ok(segment.segment_sequence)
    }

    pub(super) fn complete_segment_continuation(
        &mut self,
    ) -> Result<(), DeveloperLoopError> {
        let Some(segment) = self.segment_continuation.clone() else {
            return Ok(());
        };
        self.commit(
            &MemoryRecord::InvocationCompleted {
                schema_version: SEGMENT_CONTINUATION_SCHEMA_VERSION,
                invocation: segment.invocation,
                request_prefix: segment.request_prefix,
                segment_sequence: segment.segment_sequence,
            },
            &[],
        )?;
        self.segment_continuation = None;
        Ok(())
    }

    fn current_inputs(&self) -> Result<Vec<Message>, DeveloperLoopError> {
        let mut messages = Vec::new();
        for sequence in &self.transcript.current_inputs {
            let mut decoded =
                decode_messages(&self.artifact(*sequence)?, ProtocolLimits::PRODUCTION)?;
            if decoded.len() != 1 {
                return Err(error("durable invocation input is not one message"));
            }
            messages.push(decoded.remove(0));
        }
        Ok(messages)
    }
}

pub(super) fn protocol_limits_sha256() -> Result<[u8; 32], DeveloperLoopError> {
    let bytes = serde_json::to_vec(&ProtocolLimits::PRODUCTION.as_array())
        .map_err(|_| error("encode selected protocol limits"))?;
    Ok(peritus_codec::sha256(&bytes).into_bytes())
}

fn expected_request_digest(
    prefix: &str,
    segment: u32,
    turn: u16,
    attempt: u64,
) -> [u8; 32] {
    let prefix = DeveloperLoopLimits::request_prefix_for_segment(prefix, segment);
    peritus_codec::sha256(format!("{prefix}-{turn}-attempt-{attempt}").as_bytes()).into_bytes()
}
