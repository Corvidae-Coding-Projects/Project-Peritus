//! Deterministic host protocol projection; recovered proposals never execute here.

use super::super::super::{
    error,
    record::{ArchiveKind, ArchivedObservation, PendingDescriptor, PendingState},
};
use super::{LocalMemory, call_identity, environment};
use peritus_agent::DeveloperLoopError;
use peritus_context::{
    ContextNodeId,
    working::{
        ObservationId, PendingOperationState, WorkingEvent, WorkingPendingOperation,
        WorkingProtocol, WorkingProtocolUpdate,
    },
};
use peritus_model_protocol::{ContentBlock, ProtocolLimits, decode_messages};
use serde_json::Value;

impl LocalMemory {
    pub(in crate::local_context) fn project_observation(
        &mut self,
        source: &ArchivedObservation,
    ) -> Result<(), DeveloperLoopError> {
        if source.invocation != self.transcript.invocation {
            return Err(error("source projection invocation mismatch"));
        }
        match source.kind {
            ArchiveKind::Policy
            | ArchiveKind::User
            | ArchiveKind::Assistant
            | ArchiveKind::ToolMessage => {
                if !self.transcript.message_ids.contains(&source.sequence) {
                    self.transcript.message_ids.push(source.sequence);
                }
                if source.kind == ArchiveKind::Assistant {
                    let messages = decode_messages(
                        &self.store.read(source.artifact)?,
                        ProtocolLimits::PRODUCTION,
                    )?;
                    for message in messages {
                        for block in message.content() {
                            if let ContentBlock::ToolCall(call) = block {
                                let identity = call_identity(call);
                                let key = environment::key(
                                    format!("proposal:{}/{}", source.invocation, identity.id)
                                        .as_bytes(),
                                )?
                                .into_bytes();
                                if self.transcript.pending.iter().any(|pending| pending.key == key)
                                {
                                    return Err(error("duplicate pending proposal identity"));
                                }
                                self.transcript.pending.push(PendingDescriptor {
                                    key,
                                    invocation: source.invocation,
                                    call: identity,
                                    source: source.sequence,
                                    handle: None,
                                    state: PendingState::Proposed,
                                });
                            }
                        }
                    }
                }
            }
            ArchiveKind::ToolOutput => self.project_tool(source)?,
        }
        self.transcript.pending.sort_by_key(|pending| pending.key);
        Ok(())
    }

    fn project_tool(&mut self, source: &ArchivedObservation) -> Result<(), DeveloperLoopError> {
        let call =
            source.call.as_ref().ok_or_else(|| error("tool observation has no call identity"))?;
        self.transcript.pending.retain(|pending| {
            !(pending.invocation == source.invocation
                && pending.call.id == call.id
                && pending.handle.is_none())
        });
        if !matches!(
            call.name.as_str(),
            "command_start"
                | "command_poll"
                | "command_stdin"
                | "command_resize"
                | "command_signal"
                | "command_cancel"
                | "command_recover"
        ) {
            return Ok(());
        }
        let value: Value = serde_json::from_slice(&self.store.read(source.artifact)?)
            .map_err(|_| error("decode canonical tool observation"))?;
        if let Some(handle) = value.get("handle").and_then(Value::as_str) {
            if handle.len() > 256 {
                return Err(error("operation handle exceeds bound"));
            }
            let key = environment::key(format!("operation:{handle}").as_bytes())?.into_bytes();
            self.transcript.pending.retain(|pending| pending.key != key);
            let state = match value.get("state").and_then(Value::as_str) {
                Some("running") => Some(PendingState::Running),
                Some("completed") => None,
                _ => Some(PendingState::Unknown),
            };
            if let Some(state) = state {
                self.transcript.pending.push(PendingDescriptor {
                    key,
                    invocation: source.invocation,
                    call: call.clone(),
                    source: source.sequence,
                    handle: Some(handle.to_owned()),
                    state,
                });
            }
        }
        Ok(())
    }

    pub(in crate::local_context) fn sync_protocol(&mut self) -> Result<(), DeveloperLoopError> {
        let requirements = self
            .transcript
            .message_ids
            .iter()
            .filter_map(|id| self.sources.iter().find(|source| source.sequence == *id))
            .filter(|source| matches!(source.kind, ArchiveKind::Policy | ArchiveKind::User))
            .map(|source| {
                ObservationId::new(source.sequence).map_err(|_| error("invalid literal source"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let pending = self
            .transcript
            .pending
            .iter()
            .map(|pending| {
                Ok(WorkingPendingOperation::new(
                    ContextNodeId::new(pending.key)
                        .map_err(|_| error("invalid pending identity"))?,
                    ObservationId::new(pending.source)
                        .map_err(|_| error("invalid pending source"))?,
                    match pending.state {
                        PendingState::Proposed => PendingOperationState::Proposed,
                        PendingState::Running => PendingOperationState::Running,
                        PendingState::Unknown => PendingOperationState::Unknown,
                    },
                ))
            })
            .collect::<Result<Vec<_>, DeveloperLoopError>>()?;
        let protocol = WorkingProtocol::new(requirements, pending, self.limits)
            .map_err(|_| error("invalid semantic pins"))?;
        if self
            .state
            .protocol(self.state.binding())
            .map_err(|_| error("protocol scope mismatch"))?
            != &protocol
        {
            self.state_event(&WorkingEvent::Protocol(WorkingProtocolUpdate::new(
                self.state.binding(),
                self.state.revision(),
                protocol,
            )))?;
        }
        Ok(())
    }
}
