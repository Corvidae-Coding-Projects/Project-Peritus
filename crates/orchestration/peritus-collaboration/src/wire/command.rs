//! Canonical family-73 collaboration command codec.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use crate::{CollaborationCommand, CollaborationCommandKind};

/// Canonical family-73 schema-v1 collaboration command frame.
pub struct CollaborationCommandFrame(pub CollaborationCommand);

impl CollaborationCommandFrame {
    pub fn from_command(command: &CollaborationCommand) -> Self {
        Self(command.clone())
    }
    #[cfg(test)]
    pub fn into_command(self) -> CollaborationCommand {
        self.0
    }
}

impl CanonicalEncode for CollaborationCommandFrame {
    const FAMILY: u16 = 73;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let command = &self.0;
        super::write_id(writer, command.command_id().as_bytes())?;
        super::write_id(writer, command.event_id().as_bytes())?;
        super::write_id(writer, command.run_id().as_bytes())?;
        writer.write_u64(command.expected_sequence())?;
        writer.write_option_tag(command.expected_previous_event().is_some())?;
        if let Some(previous) = command.expected_previous_event() {
            super::write_id(writer, previous.as_bytes())?;
        }
        super::write_digest(writer, command.prior_state_digest())?;
        super::write_revision(writer, command.revision())?;
        write_kind(writer, command.kind())
    }
}

impl CanonicalDecode for CollaborationCommandFrame {
    const FAMILY: u16 = 73;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::read_command_id(reader)?;
        let event_id = super::read_event_id(reader)?;
        let run_id = super::read_run_id(reader)?;
        let sequence = reader.read_u64()?;
        let previous =
            reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
        if (sequence == 0) != previous.is_none() {
            return Err(super::invalid(reader));
        }
        CollaborationCommand::new(
            command_id,
            event_id,
            run_id,
            sequence,
            previous,
            super::read_digest(reader)?,
            super::read_revision(reader)?,
            read_kind(reader)?,
        )
        .map(Self)
        .map_err(|_| super::invalid(reader))
    }
}

#[allow(clippy::too_many_lines, reason = "closed wire tag table stays contiguous")]
pub(super) fn write_kind(
    writer: &mut CanonicalWriter,
    kind: &CollaborationCommandKind,
) -> Result<(), CodecError> {
    match kind {
        CollaborationCommandKind::Start { binding } => {
            writer.write_u8(1)?;
            super::write_binding(writer, binding)?;
        }
        CollaborationCommandKind::OfferDelegation { offered_by, assignment } => {
            writer.write_u8(2)?;
            super::write_id(writer, offered_by.as_bytes())?;
            super::write_delegation(writer, assignment)?;
        }
        CollaborationCommandKind::AcceptDelegation { task_id, accepted_by } => {
            writer.write_u8(3)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, accepted_by.as_bytes())?;
        }
        CollaborationCommandKind::RejectDelegation { task_id, rejected_by, reason_digest } => {
            writer.write_u8(4)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, rejected_by.as_bytes())?;
            super::write_digest(writer, *reason_digest)?;
        }
        CollaborationCommandKind::ActivateTask { task_id, observation } => {
            writer.write_u8(5)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_reservation(writer, *observation)?;
        }
        CollaborationCommandKind::SendMessage { message } => {
            writer.write_u8(6)?;
            super::write_message(writer, message)?;
        }
        CollaborationCommandKind::AcknowledgeMessage { message_id, receiver } => {
            writer.write_u8(7)?;
            super::write_id(writer, message_id.as_bytes())?;
            super::write_id(writer, receiver.as_bytes())?;
        }
        CollaborationCommandKind::CompleteTask { task_id, completed_by, terminal } => {
            writer.write_u8(8)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, completed_by.as_bytes())?;
            super::write_task_terminal(writer, *terminal)?;
        }
        CollaborationCommandKind::AbandonTask { task_id, abandoned_by, reason_digest } => {
            writer.write_u8(9)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, abandoned_by.as_bytes())?;
            super::write_digest(writer, *reason_digest)?;
        }
        CollaborationCommandKind::CancelTask { task_id, requested_by, reason_digest } => {
            writer.write_u8(10)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, requested_by.as_bytes())?;
            super::write_digest(writer, *reason_digest)?;
        }
        CollaborationCommandKind::AcknowledgeCancellation { task_id, owner } => {
            writer.write_u8(11)?;
            super::write_id(writer, task_id.as_bytes())?;
            super::write_id(writer, owner.as_bytes())?;
        }
        CollaborationCommandKind::Pause { requested_by } => {
            writer.write_u8(12)?;
            super::write_id(writer, requested_by.as_bytes())?;
        }
        CollaborationCommandKind::Resume { requested_by } => {
            writer.write_u8(13)?;
            super::write_id(writer, requested_by.as_bytes())?;
        }
        CollaborationCommandKind::Finalize => writer.write_u8(14)?,
    }
    Ok(())
}

#[allow(clippy::too_many_lines, reason = "closed wire tag table stays contiguous")]
pub(super) fn read_kind(
    reader: &mut CanonicalReader<'_>,
) -> Result<CollaborationCommandKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
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
        10 => Ok(CollaborationCommandKind::CancelTask {
            task_id: super::read_task_id(reader)?,
            requested_by: super::read_actor_id(reader)?,
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
