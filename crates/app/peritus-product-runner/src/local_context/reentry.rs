//! Exact invocation reopening for a durably scheduled definitely-unaccepted request.

use peritus_agent::{
    DeveloperContextResume, DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopProgress,
    DeveloperLoopRequest,
};
use peritus_model_protocol::{
    CompletedToolCall, ContentBlock, Message, ProtocolLimits, Role, decode_messages,
    encode_messages,
};

use super::{
    error,
    memory::LocalMemory,
    record::{
        ArchiveKind, CallIdentity, LEGACY_SEGMENT_CONTINUATION_SCHEMA_VERSION, MemoryRecord,
        SEGMENT_CONTINUATION_SCHEMA_VERSION, SegmentContinuation,
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
        self.pending_reentry_prefixes(&[expected_prefix])
    }

    pub(super) fn pending_reentry_prefixes(
        &self,
        expected_prefixes: &[&str],
    ) -> Result<Option<String>, DeveloperLoopError> {
        Ok(self
            .pending_developer_reentry_prefixes(expected_prefixes)?
            .map(|(request_prefix, _)| request_prefix))
    }

    pub(super) fn pending_developer_reentry(
        &self,
        expected_prefix: &str,
    ) -> Result<Option<(String, u64)>, DeveloperLoopError> {
        self.pending_developer_reentry_prefixes(&[expected_prefix])
    }

    fn pending_developer_reentry_prefixes(
        &self,
        expected_prefixes: &[&str],
    ) -> Result<Option<(String, u64)>, DeveloperLoopError> {
        if expected_prefixes.is_empty() || expected_prefixes.iter().any(|prefix| prefix.is_empty()) {
            return Err(error("pending reentry has no accepted logical invocation identity"));
        }
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
        if !expected_prefixes.iter().any(|expected_prefix| {
            super::grounding::valid_invocation(&request_prefix, expected_prefix)
        }) {
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
        if segment.and_then(|segment| segment.pending_batch.as_ref()).is_none()
            && self.transcript.pending.iter().any(|pending| pending.handle.is_none())
        {
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
        let (turn, retries) = match pending.as_ref() {
            Some(pending) => (
                pending.turn(),
                pending
                    .attempt()
                    .checked_sub(1)
                    .ok_or_else(|| error("pending retry attempt identity is zero"))?,
            ),
            None => (1, 0),
        };
        let inputs = if segment.is_some() {
            durable_inputs.as_slice()
        } else {
            initial_messages
        };
        let resume = if let Some(segment) = segment {
            let progress = DeveloperLoopProgress::new(
                segment.progress.model_turns,
                segment.progress.tool_calls,
                segment.progress.compactions,
                segment.progress.retries,
            );
            let completed_tool_calls = u64::try_from(
                self.sources
                    .iter()
                    .filter(|source| {
                        Some(source.invocation) == invocation
                            && source.kind == ArchiveKind::ToolOutput
                    })
                    .count(),
            )
            .map_err(|_| error("durable invocation tool count overflow"))?;
            let physical_completed = if segment.schema_version
                == LEGACY_SEGMENT_CONTINUATION_SCHEMA_VERSION
            {
                0
            } else {
                completed_tool_calls
                    .checked_sub(progress.tool_calls())
                    .ok_or_else(|| error("segment tool progress exceeds durable observations"))?
            };
            let (pending_exchange, pending_calls, pending_first_sequence, admitted_batch) =
                self.pending_batch_replay(segment)?;
            let physical_tool_calls = match admitted_batch {
                Some(admitted) => {
                    if physical_completed > u64::from(admitted) {
                        return Err(error("pending batch tool frontier is not contiguous"));
                    }
                    admitted
                }
                None => u32::try_from(physical_completed)
                    .map_err(|_| error("physical segment tool count overflow"))?,
            };
            DeveloperContextResume::new_segment_with_progress(
                turn,
                inputs,
                self.last_view.clone(),
                physical_tool_calls,
                retries,
                progress,
                pending_exchange,
                pending_calls,
                pending_first_sequence,
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
    ) -> Result<u64, DeveloperLoopError> {
        let Some(segment) = self.segment_continuation.as_ref() else {
            return Ok(0);
        };
        if !valid_segment_schema(segment.schema_version)
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
                schema_version: segment.schema_version,
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

    fn pending_batch_replay(
        &self,
        segment: &SegmentContinuation,
    ) -> Result<(Vec<Message>, Vec<CompletedToolCall>, u32, Option<u32>), DeveloperLoopError> {
        let Some(batch) = segment.pending_batch.as_ref() else {
            return Ok((Vec::new(), Vec::new(), 1, None));
        };
        if segment.schema_version != SEGMENT_CONTINUATION_SCHEMA_VERSION
            || batch.assistant_source == 0
            || batch.calls.is_empty()
        {
            return Err(error("invalid durable pending tool batch"));
        }
        let source = self
            .sources
            .iter()
            .find(|source| source.sequence == batch.assistant_source)
            .ok_or_else(|| error("pending batch assistant source is unavailable"))?;
        if source.invocation != segment.invocation || source.kind != ArchiveKind::Assistant {
            return Err(error("pending batch assistant source identity mismatch"));
        }
        let mut decoded =
            decode_messages(&self.artifact(source.sequence)?, ProtocolLimits::PRODUCTION)?;
        if decoded.len() != 1 || decoded[0].role() != Role::Assistant {
            return Err(error("pending batch source is not one assistant message"));
        }
        let assistant = decoded.remove(0);
        let calls = assistant
            .content()
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolCall(call) => Some(call.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let identities = calls.iter().map(call_identity).collect::<Vec<_>>();
        if identities != batch.calls {
            return Err(error("pending batch call identities changed"));
        }

        let mut results = Vec::<(String, Message)>::new();
        for message_source in self
            .transcript
            .message_ids
            .iter()
            .copied()
            .filter(|sequence| *sequence > batch.assistant_source)
        {
            let mut messages =
                decode_messages(&self.artifact(message_source)?, ProtocolLimits::PRODUCTION)?;
            if messages.len() != 1 {
                return Err(error("pending batch result source is not one message"));
            }
            let message = messages.remove(0);
            if message.role() != Role::Tool {
                continue;
            }
            for block in message.content() {
                if let ContentBlock::ToolResult(result) = block {
                    let id = result.call_id().expose_for_wire().to_owned();
                    if results.iter().any(|(existing, _)| existing == &id) {
                        return Err(error("pending batch has duplicate durable tool results"));
                    }
                    results.push((id, message.clone()));
                }
            }
        }

        let mut replay = vec![assistant];
        let mut completed = 0_usize;
        let mut saw_gap = false;
        for call in &calls {
            let result = results
                .iter()
                .find(|(id, _)| id == call.id().expose_for_wire())
                .map(|(_, message)| message.clone());
            match result {
                Some(_) if saw_gap => {
                    return Err(error("pending batch durable result frontier has a gap"));
                }
                Some(message) => {
                    replay.push(message);
                    completed = completed
                        .checked_add(1)
                        .ok_or_else(|| error("pending batch result frontier overflow"))?;
                }
                None => saw_gap = true,
            }
        }
        if results.len() != completed {
            return Err(error("pending batch contains an unrelated tool result"));
        }
        let remaining = calls[completed..].to_vec();
        for call in &remaining {
            let identity = call_identity(call);
            let proposed = self.transcript.pending.iter().any(|pending| {
                pending.invocation == segment.invocation
                    && pending.call == identity
                    && pending.handle.is_none()
            });
            let observed = self.sources.iter().any(|source| {
                source.invocation == segment.invocation
                    && source.kind == ArchiveKind::ToolOutput
                    && source.call.as_ref() == Some(&identity)
            });
            if !proposed && !observed {
                return Err(error("pending batch call has no durable proposal or recovered output"));
            }
        }
        let admitted = u32::try_from(calls.len())
            .map_err(|_| error("pending batch admission count overflow"))?;
        let first_sequence = u32::try_from(completed)
            .map_err(|_| error("pending batch result frontier overflow"))?
            .checked_add(1)
            .ok_or_else(|| error("pending batch result frontier overflow"))?;
        Ok((replay, remaining, first_sequence, Some(admitted)))
    }
}

fn valid_segment_schema(schema_version: u16) -> bool {
    matches!(
        schema_version,
        LEGACY_SEGMENT_CONTINUATION_SCHEMA_VERSION | SEGMENT_CONTINUATION_SCHEMA_VERSION
    )
}

fn call_identity(call: &CompletedToolCall) -> CallIdentity {
    CallIdentity {
        id: call.id().expose_for_wire().to_owned(),
        name: call.name().as_str().to_owned(),
        arguments_digest: peritus_codec::sha256(call.arguments().canonical_bytes()).into_bytes(),
    }
}

pub(super) fn protocol_limits_sha256() -> Result<[u8; 32], DeveloperLoopError> {
    let bytes = serde_json::to_vec(&ProtocolLimits::PRODUCTION.as_array())
        .map_err(|_| error("encode selected protocol limits"))?;
    Ok(peritus_codec::sha256(&bytes).into_bytes())
}

fn expected_request_digest(
    prefix: &str,
    segment: u64,
    turn: u16,
    attempt: u64,
) -> [u8; 32] {
    let prefix = DeveloperLoopLimits::request_prefix_for_segment(prefix, segment);
    peritus_codec::sha256(format!("{prefix}-{turn}-attempt-{attempt}").as_bytes()).into_bytes()
}
