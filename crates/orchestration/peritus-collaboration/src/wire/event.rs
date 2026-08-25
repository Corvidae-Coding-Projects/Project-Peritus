//! Canonical family-74 collaboration event codec.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::EventSequence;

use crate::{
    CancellationEffect, CollaborationCommandKind, CollaborationEvent, CollaborationEventKind,
    TaskPhase,
};

/// Canonical family-74 schema-v1 collaboration event frame.
pub struct CollaborationEventFrame(pub CollaborationEvent);

impl CollaborationEventFrame {
    pub fn into_event(self) -> CollaborationEvent {
        self.0
    }
}

impl CanonicalEncode for CollaborationEventFrame {
    const FAMILY: u16 = 74;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let event = &self.0;
        super::write_id(writer, event.id().as_bytes())?;
        super::write_id(writer, event.command_id().as_bytes())?;
        writer.write_u64(event.sequence().get())?;
        writer.write_option_tag(event.previous_event().is_some())?;
        if let Some(previous) = event.previous_event() {
            super::write_id(writer, previous.as_bytes())?;
        }
        super::write_id(writer, event.run_id().as_bytes())?;
        super::write_revision(writer, event.revision())?;
        super::write_digest(writer, event.prior_state_digest())?;
        super::write_digest(writer, event.successor_state_digest())?;
        write_kind(writer, event.kind())
    }
}

impl CanonicalDecode for CollaborationEventFrame {
    const FAMILY: u16 = 74;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = super::read_event_id(reader)?;
        let command_id = super::read_command_id(reader)?;
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let previous =
            reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
        if (sequence.get() == 1) != previous.is_none() {
            return Err(super::invalid(reader));
        }
        Ok(Self(CollaborationEvent::from_wire(
            id,
            command_id,
            sequence,
            previous,
            super::read_run_id(reader)?,
            super::read_revision(reader)?,
            super::read_digest(reader)?,
            super::read_digest(reader)?,
            read_kind(reader)?,
        )))
    }
}

fn write_kind(
    writer: &mut CanonicalWriter,
    kind: &CollaborationEventKind,
) -> Result<(), CodecError> {
    match kind {
        CollaborationEventKind::CancellationPropagated {
            task_id,
            requested_by,
            reason_digest,
            effects,
        } => {
            writer.write_u8(10)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, requested_by.as_bytes())?;
            super::write_digest(writer, *reason_digest)?;
            writer.write_collection_len(effects.len())?;
            for effect in effects {
                super::write_id(writer, effect.task_id().as_bytes())?;
                writer.write_u8(crate::canonical::task_phase_tag(effect.resulting_phase()))?;
            }
            Ok(())
        }
        other => command::write_kind(writer, &command_kind(other)?),
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<CollaborationEventKind, CodecError> {
    let offset = reader.offset();
    let tag = reader.read_u8()?;
    if tag == 10 {
        let task_id = super::read_task_id(reader)?;
        let requested_by = super::read_actor_id(reader)?;
        let reason_digest = super::read_digest(reader)?;
        let count = super::bounded_len(reader, crate::CollaborationLimits::MAX_TASKS as usize)?;
        if count == 0 {
            return Err(super::invalid(reader));
        }
        let mut effects = Vec::with_capacity(count);
        for _ in 0..count {
            let id = super::read_task_id(reader)?;
            let phase_offset = reader.offset();
            let phase = match reader.read_u8()? {
                4 => TaskPhase::Cancelling,
                5 => TaskPhase::Terminal,
                _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, phase_offset)),
            };
            effects.push(CancellationEffect::new(id, phase));
        }
        if effects.windows(2).any(|pair| pair[0].task_id() >= pair[1].task_id()) {
            return Err(super::invalid(reader));
        }
        return Ok(CollaborationEventKind::CancellationPropagated {
            task_id,
            requested_by,
            reason_digest,
            effects,
        });
    }
    event_from_command(read_command_after_tag(reader, offset, tag)?)
}

fn read_command_after_tag(
    reader: &mut CanonicalReader<'_>,
    offset: usize,
    tag: u8,
) -> Result<CollaborationCommandKind, CodecError> {
    match tag {
        1 => Ok(CollaborationCommandKind::Start { binding: super::read_binding(reader)? }),
        2 => Ok(CollaborationCommandKind::OfferDelegation {
            offered_by: super::read_actor_id(reader)?,
            assignment: super::read_delegation(reader)?,
        }),
        3 => Ok(CollaborationCommandKind::AcceptDelegation {
            task_id: super::read_task_id(reader)?,
            accepted_by: super::read_actor_id(reader)?,
        }),
        4 => Ok(CollaborationCommandKind::RejectDelegation {
            task_id: super::read_task_id(reader)?,
            rejected_by: super::read_actor_id(reader)?,
            reason_digest: super::read_digest(reader)?,
        }),
        5 => Ok(CollaborationCommandKind::ActivateTask {
            task_id: super::read_task_id(reader)?,
            observation: super::read_reservation(reader)?,
        }),
        6 => Ok(CollaborationCommandKind::SendMessage { message: super::read_message(reader)? }),
        7 => Ok(CollaborationCommandKind::AcknowledgeMessage {
            message_id: super::read_message_id(reader)?,
            receiver: super::read_actor_id(reader)?,
        }),
        8 => Ok(CollaborationCommandKind::CompleteTask {
            task_id: super::read_task_id(reader)?,
            completed_by: super::read_actor_id(reader)?,
            terminal: super::read_task_terminal(reader)?,
        }),
        9 => Ok(CollaborationCommandKind::AbandonTask {
            task_id: super::read_task_id(reader)?,
            abandoned_by: super::read_actor_id(reader)?,
            reason_digest: super::read_digest(reader)?,
        }),
        11 => Ok(CollaborationCommandKind::AcknowledgeCancellation {
            task_id: super::read_task_id(reader)?,
            owner: super::read_actor_id(reader)?,
        }),
        12 => Ok(CollaborationCommandKind::Pause { requested_by: super::read_actor_id(reader)? }),
        13 => Ok(CollaborationCommandKind::Resume { requested_by: super::read_actor_id(reader)? }),
        14 => Ok(CollaborationCommandKind::Finalize),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn event_from_command(
    kind: CollaborationCommandKind,
) -> Result<CollaborationEventKind, CodecError> {
    Ok(match kind {
        CollaborationCommandKind::Start { binding } => CollaborationEventKind::Started { binding },
        CollaborationCommandKind::OfferDelegation { offered_by, assignment } => {
            CollaborationEventKind::DelegationOffered { offered_by, assignment }
        }
        CollaborationCommandKind::AcceptDelegation { task_id, accepted_by } => {
            CollaborationEventKind::DelegationAccepted { task_id, accepted_by }
        }
        CollaborationCommandKind::RejectDelegation { task_id, rejected_by, reason_digest } => {
            CollaborationEventKind::DelegationRejected { task_id, rejected_by, reason_digest }
        }
        CollaborationCommandKind::ActivateTask { task_id, observation } => {
            CollaborationEventKind::TaskActivated { task_id, observation }
        }
        CollaborationCommandKind::SendMessage { message } => {
            CollaborationEventKind::MessageSent { message }
        }
        CollaborationCommandKind::AcknowledgeMessage { message_id, receiver } => {
            CollaborationEventKind::MessageAcknowledged { message_id, receiver }
        }
        CollaborationCommandKind::CompleteTask { task_id, completed_by, terminal } => {
            CollaborationEventKind::TaskCompleted { task_id, completed_by, terminal }
        }
        CollaborationCommandKind::AbandonTask { task_id, abandoned_by, reason_digest } => {
            CollaborationEventKind::TaskAbandoned { task_id, abandoned_by, reason_digest }
        }
        CollaborationCommandKind::AcknowledgeCancellation { task_id, owner } => {
            CollaborationEventKind::CancellationAcknowledged { task_id, owner }
        }
        CollaborationCommandKind::Pause { requested_by } => {
            CollaborationEventKind::Paused { requested_by }
        }
        CollaborationCommandKind::Resume { requested_by } => {
            CollaborationEventKind::Resumed { requested_by }
        }
        CollaborationCommandKind::Finalize => CollaborationEventKind::Finalized,
        CollaborationCommandKind::CancelTask { .. } => {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0));
        }
    })
}

fn command_kind(kind: &CollaborationEventKind) -> Result<CollaborationCommandKind, CodecError> {
    Ok(match kind {
        CollaborationEventKind::Started { binding } => {
            CollaborationCommandKind::Start { binding: binding.clone() }
        }
        CollaborationEventKind::DelegationOffered { offered_by, assignment } => {
            CollaborationCommandKind::OfferDelegation {
                offered_by: *offered_by,
                assignment: assignment.clone(),
            }
        }
        CollaborationEventKind::DelegationAccepted { task_id, accepted_by } => {
            CollaborationCommandKind::AcceptDelegation {
                task_id: *task_id,
                accepted_by: *accepted_by,
            }
        }
        CollaborationEventKind::DelegationRejected { task_id, rejected_by, reason_digest } => {
            CollaborationCommandKind::RejectDelegation {
                task_id: *task_id,
                rejected_by: *rejected_by,
                reason_digest: *reason_digest,
            }
        }
        CollaborationEventKind::TaskActivated { task_id, observation } => {
            CollaborationCommandKind::ActivateTask { task_id: *task_id, observation: *observation }
        }
        CollaborationEventKind::MessageSent { message } => {
            CollaborationCommandKind::SendMessage { message: message.clone() }
        }
        CollaborationEventKind::MessageAcknowledged { message_id, receiver } => {
            CollaborationCommandKind::AcknowledgeMessage {
                message_id: *message_id,
                receiver: *receiver,
            }
        }
        CollaborationEventKind::TaskCompleted { task_id, completed_by, terminal } => {
            CollaborationCommandKind::CompleteTask {
                task_id: *task_id,
                completed_by: *completed_by,
                terminal: *terminal,
            }
        }
        CollaborationEventKind::TaskAbandoned { task_id, abandoned_by, reason_digest } => {
            CollaborationCommandKind::AbandonTask {
                task_id: *task_id,
                abandoned_by: *abandoned_by,
                reason_digest: *reason_digest,
            }
        }
        CollaborationEventKind::CancellationAcknowledged { task_id, owner } => {
            CollaborationCommandKind::AcknowledgeCancellation { task_id: *task_id, owner: *owner }
        }
        CollaborationEventKind::Paused { requested_by } => {
            CollaborationCommandKind::Pause { requested_by: *requested_by }
        }
        CollaborationEventKind::Resumed { requested_by } => {
            CollaborationCommandKind::Resume { requested_by: *requested_by }
        }
        CollaborationEventKind::Finalized => CollaborationCommandKind::Finalize,
        CollaborationEventKind::CancellationPropagated { .. } => {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0));
        }
    })
}

use super::command;
