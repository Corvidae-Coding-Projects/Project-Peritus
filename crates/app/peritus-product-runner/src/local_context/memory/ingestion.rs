//! Persist-before-observe source ingestion and exact invocation input pinning.

mod facts;
mod pending;

use super::super::{
    error,
    record::{ArchiveKind, ArchivedObservation, CallIdentity, MemoryRecord, PendingState},
};
use super::{LocalMemory, environment};
use peritus_agent::{DeveloperLoopError, DeveloperLoopRequest, DeveloperToolObservation};
use peritus_codec::sha256;
use peritus_context::working::{
    ObservationId, ObservationSource, WorkingEvent, apply_working_event, encode_working_event,
};
use peritus_model_protocol::{
    CompletedToolCall, ContentBlock, Message, ProtocolLimits, Role, encode_messages,
};

impl LocalMemory {
    pub(in crate::local_context) fn begin(
        &mut self,
        request: &DeveloperLoopRequest,
        initial: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        self.cancellation.clone_from(&request.cancellation);
        self.task_contract.clone_from(&request.prompt);
        self.refresh()?;
        let sequence = self
            .transcript
            .invocation
            .checked_add(1)
            .ok_or_else(|| error("invocation sequence overflow"))?;
        self.validate_next_invocation(sequence)?;
        self.commit(
            &MemoryRecord::Invocation { sequence, request_prefix: request.request_prefix.clone() },
            &[],
        )?;
        self.adopt_invocation(sequence, request.request_prefix.clone())?;
        self.transcript.invocation = sequence;
        self.transcript.request_prefix.clone_from(&request.request_prefix);
        self.transcript.current_inputs.clear();
        self.transcript.message_ids.clear();
        self.segment_continuation = None;
        self.persist_transcript()?;
        for message in initial {
            let id = self.observe_message(message)?;
            self.transcript.current_inputs.push(id);
        }
        self.persist_transcript()?;
        self.sync_protocol()?;
        self.tools.clone_from(&request.tools);
        Ok(())
    }

    pub(in crate::local_context) fn refresh(&mut self) -> Result<(), DeveloperLoopError> {
        let projected = environment::projected_paths(&self.state, &self.transcript.files, &[])?;
        let environment = environment::capture(
            &self.workspace,
            self.binding,
            &projected,
            &self.task_contract,
            self.limits,
            &self.workspace_scope,
            &self.cancellation,
        )?;
        if &environment != self.state.environment() {
            self.state_event(&WorkingEvent::Refresh {
                base_revision: self.state.revision(),
                environment,
            })?;
        }
        Ok(())
    }

    pub(in crate::local_context) fn observe_message(
        &mut self,
        message: &Message,
    ) -> Result<u64, DeveloperLoopError> {
        // Assistant reasoning is bounded protocol replay, retained exactly with its exchange.
        // It is never projected into authoritative instructions or derived working entries.
        if message.content().iter().any(|block| {
            matches!(block, ContentBlock::ProviderExtension(_))
                || (matches!(block, ContentBlock::Reasoning(_))
                    && message.role() != Role::Assistant)
        }) {
            return Err(error("opaque provider content is not local working memory"));
        }
        let kind = match message.role() {
            Role::System | Role::Developer => ArchiveKind::Policy,
            Role::User => ArchiveKind::User,
            Role::Assistant => ArchiveKind::Assistant,
            Role::Tool => ArchiveKind::ToolMessage,
        };
        let bytes = encode_messages(std::slice::from_ref(message), ProtocolLimits::PRODUCTION)?;
        let observation = self.persist_observation(kind, &bytes, None, None, false)?;
        self.project_observation(&observation)?;
        self.sync_protocol()?;
        Ok(observation.sequence)
    }

    pub(in crate::local_context) fn observe_tool(
        &mut self,
        call: &CompletedToolCall,
        output: &DeveloperToolObservation,
    ) -> Result<u64, DeveloperLoopError> {
        let identity = call_identity(call);
        if let Some(prior) = self.sources.iter().find(|source| {
            source.invocation == self.transcript.invocation
                && source.kind == ArchiveKind::ToolOutput
                && source.call.as_ref() == Some(&identity)
        }) {
            if prior.artifact.digest != sha256(output.output.canonical_bytes()).into_bytes()
                || prior.is_error != output.is_error
            {
                return Err(error("conflicting recovery of tool observation identity"));
            }
            return Ok(prior.sequence);
        }
        self.observe_tool_in(self.transcript.invocation, self.next_tool_sequence()?, call, output)
    }

    pub(in crate::local_context) fn mark_tool_effect_uncertain(
        &mut self,
        tool_sequence: u64,
        call: &CompletedToolCall,
    ) -> Result<(), DeveloperLoopError> {
        let identity = call_identity(call);
        if tool_sequence != self.next_tool_sequence()? {
            return Err(error("tool effect boundary sequence is not contiguous"));
        }
        let pending = self
            .transcript
            .pending
            .iter()
            .find(|pending| {
                pending.invocation == self.transcript.invocation
                    && pending.call == identity
                    && pending.handle.is_none()
            })
            .ok_or_else(|| error("tool effect boundary has no exact durable proposal"))?;
        if pending.state != PendingState::Proposed {
            return Err(error("tool effect boundary is already unresolved"));
        }
        let invocation = self.transcript.invocation;
        let request_prefix = self.transcript.request_prefix.clone();
        self.commit(
            &MemoryRecord::ToolEffectUncertain {
                invocation,
                request_prefix: request_prefix.clone(),
                tool_sequence,
                call: identity.clone(),
            },
            &[],
        )?;
        self.apply_tool_effect_uncertain(
            invocation,
            &request_prefix,
            tool_sequence,
            &identity,
        )?;
        self.sync_protocol()
    }

    pub(super) fn apply_tool_effect_uncertain(
        &mut self,
        invocation: u64,
        request_prefix: &str,
        tool_sequence: u64,
        call: &CallIdentity,
    ) -> Result<(), DeveloperLoopError> {
        if invocation != self.transcript.invocation
            || request_prefix != self.transcript.request_prefix
            || tool_sequence != self.next_tool_sequence()?
        {
            return Err(error("tool effect boundary identity is not current"));
        }
        let mut matched = 0_usize;
        for pending in &mut self.transcript.pending {
            if pending.invocation == invocation
                && &pending.call == call
                && pending.handle.is_none()
            {
                if pending.state != PendingState::Proposed {
                    return Err(error("tool effect boundary conflicts with pending state"));
                }
                pending.state = PendingState::Unknown;
                matched = matched
                    .checked_add(1)
                    .ok_or_else(|| error("tool effect boundary match count overflow"))?;
            }
        }
        if matched != 1 {
            return Err(error("tool effect boundary does not identify one proposal"));
        }
        self.prepared = None;
        Ok(())
    }

    fn next_tool_sequence(&self) -> Result<u64, DeveloperLoopError> {
        self.sources
            .iter()
            .rev()
            .find(|source| {
                source.invocation == self.transcript.invocation
                    && source.kind == ArchiveKind::ToolOutput
            })
            .and_then(|source| source.tool_sequence)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| error("tool sequence overflow"))
    }

    pub(in crate::local_context) fn observe_tool_in(
        &mut self,
        invocation: u64,
        tool_sequence: u64,
        call: &CompletedToolCall,
        output: &DeveloperToolObservation,
    ) -> Result<u64, DeveloperLoopError> {
        let identity = call_identity(call);
        let bytes = output.output.canonical_bytes();
        if let Some(prior) = self.sources.iter().find(|source| {
            source.invocation == invocation && source.tool_sequence == Some(tool_sequence)
        }) {
            if prior.call.as_ref() != Some(&identity)
                || prior.artifact.digest != sha256(bytes)
                || prior.is_error != output.is_error
            {
                return Err(error("conflicting reuse of tool observation identity"));
            }
            return Ok(prior.sequence);
        }
        if invocation != self.transcript.invocation {
            return Err(error("recovery tool observation belongs to another invocation"));
        }
        if tool_sequence != self.next_tool_sequence()? {
            return Err(error("tool observation sequence is not contiguous"));
        }
        if !self
            .transcript
            .pending
            .iter()
            .any(|pending| pending.invocation == invocation && pending.call == identity)
        {
            return Err(error("tool observation has no matching durable proposal"));
        }
        let observation = self.persist_observation(
            ArchiveKind::ToolOutput,
            bytes,
            Some(tool_sequence),
            Some(identity),
            output.is_error,
        )?;
        self.project_observation(&observation)?;
        self.sync_protocol()?;
        self.ingest_facts()?;
        Ok(observation.sequence)
    }

    fn persist_observation(
        &mut self,
        kind: ArchiveKind,
        bytes: &[u8],
        tool_sequence: Option<u64>,
        call: Option<CallIdentity>,
        is_error: bool,
    ) -> Result<ArchivedObservation, DeveloperLoopError> {
        let sequence = self
            .state
            .through_observation()
            .checked_add(1)
            .ok_or_else(|| error("observation sequence overflow"))?;
        let artifact = self.store.store(bytes)?;
        let source = ObservationSource::new(
            ObservationId::new(sequence).map_err(|_| error("invalid observation sequence"))?,
            artifact.digest,
            artifact.bytes,
            0,
            artifact.bytes,
            kind.source_kind(),
        )
        .map_err(|_| error("invalid observation source range"))?;
        let event = WorkingEvent::Observation { binding: self.state.binding(), source };
        let next = apply_working_event(&self.state, &event)
            .map_err(|_| error("observation ingestion rejected"))?;
        let reducer = self
            .store
            .store(&encode_working_event(&event).map_err(|_| error("encode observation event"))?)?;
        let observation = ArchivedObservation {
            sequence,
            invocation: self.transcript.invocation,
            tool_sequence,
            kind,
            artifact,
            call,
            is_error,
        };
        self.commit(
            &MemoryRecord::Observation { observation: observation.clone(), reducer },
            &[artifact.digest, reducer.digest],
        )?;
        self.state = next;
        self.sources.push(observation.clone());
        self.prepared = None;
        Ok(observation)
    }

    pub(in crate::local_context) fn persist_transcript(
        &mut self,
    ) -> Result<(), DeveloperLoopError> {
        self.publish_transcript_index()
    }
}

pub(in crate::local_context) fn call_identity(call: &CompletedToolCall) -> CallIdentity {
    CallIdentity {
        id: call.id().expose_for_wire().to_owned(),
        name: call.name().as_str().to_owned(),
        arguments_digest: sha256(call.arguments().canonical_bytes()).into_bytes(),
    }
}
