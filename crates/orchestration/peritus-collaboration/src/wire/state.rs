//! Canonical family-75 complete collaboration-state codec.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::EventSequence;

use crate::{
    CollaborationPhase, CollaborationState, CollaborationTask, CollaborationTerminal,
    CollaborationTerminalKind, MessageDelivery, TaskPhase,
};

/// Canonical family-75 schema-v1 complete collaboration checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationStateFrame(CollaborationState);

impl CollaborationStateFrame {
    pub fn from_state(state: &CollaborationState) -> Self {
        Self(state.clone())
    }
    pub fn matches_state(&self, state: &CollaborationState) -> bool {
        &self.0 == state
    }
    pub const fn run_id(&self) -> peritus_types::RunId {
        self.0.run_id()
    }
    pub const fn sequence(&self) -> EventSequence {
        self.0.sequence()
    }
    pub const fn last_event_id(&self) -> peritus_types::EventId {
        self.0.last_event_id()
    }
    pub const fn revision(&self) -> peritus_types::RevisionTuple {
        self.0.binding().revision()
    }
    pub const fn state_digest(&self) -> peritus_types::Sha256Digest {
        self.0.state_digest()
    }
}

impl CanonicalEncode for CollaborationStateFrame {
    const FAMILY: u16 = 75;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let state = &self.0;
        super::write_binding(writer, state.binding())?;
        writer.write_u8(crate::canonical::phase_tag(state.phase()))?;
        writer.write_u64(state.sequence().get())?;
        super::write_id(writer, state.last_event_id().as_bytes())?;
        super::write_digest(writer, state.state_digest())?;
        writer.write_collection_len(state.tasks().len())?;
        for task in state.tasks() {
            write_task(writer, task)?;
        }
        writer.write_collection_len(state.messages().len())?;
        for delivery in state.messages() {
            super::write_message(writer, delivery.message())?;
            writer.write_bool(delivery.acknowledged())?;
        }
        writer.write_collection_len(state.used_commands().len())?;
        for command in state.used_commands() {
            super::write_id(writer, command.as_bytes())?;
        }
        writer.write_option_tag(state.terminal().is_some())?;
        if let Some(terminal) = state.terminal() {
            write_terminal(writer, terminal)?;
        }
        Ok(())
    }
}

impl CanonicalDecode for CollaborationStateFrame {
    const FAMILY: u16 = 75;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let binding = super::read_binding(reader)?;
        let phase_offset = reader.offset();
        let phase = match reader.read_u8()? {
            1 => CollaborationPhase::Active,
            2 => CollaborationPhase::Paused,
            3 => CollaborationPhase::Terminal,
            _ => return Err(super::unknown(phase_offset)),
        };
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let last_event_id = super::read_event_id(reader)?;
        let state_digest = super::read_digest(reader)?;
        let task_count = super::bounded_len(reader, binding.limits().tasks() as usize)?;
        if task_count == 0 {
            return Err(super::invalid(reader));
        }
        let mut tasks = Vec::with_capacity(task_count);
        for _ in 0..task_count {
            tasks.push(read_task(reader)?);
        }
        if tasks
            .windows(2)
            .any(|pair| pair[0].assignment().task_id() >= pair[1].assignment().task_id())
        {
            return Err(super::invalid(reader));
        }
        let message_count = super::bounded_len(reader, binding.limits().messages() as usize)?;
        let mut messages = Vec::with_capacity(message_count);
        for _ in 0..message_count {
            messages.push(MessageDelivery::from_wire(
                super::read_message(reader)?,
                reader.read_bool()?,
            ));
        }
        if messages.windows(2).any(|pair| pair[0].message().id() >= pair[1].message().id()) {
            return Err(super::invalid(reader));
        }
        let command_count = super::bounded_len(
            reader,
            binding
                .limits()
                .tasks()
                .saturating_add(binding.limits().messages().saturating_mul(2))
                .saturating_add(65_535) as usize,
        )?;
        let mut used_commands = Vec::with_capacity(command_count);
        for _ in 0..command_count {
            used_commands.push(super::read_command_id(reader)?);
        }
        let terminal = reader.read_option_tag()?.then(|| read_terminal(reader)).transpose()?;
        let state = CollaborationState::from_wire(
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
            tasks,
            messages,
            used_commands,
            terminal,
        );
        state.validate_inert().map_err(|_| super::invalid(reader))?;
        Ok(Self(state))
    }
}

fn write_task(writer: &mut CanonicalWriter, value: &CollaborationTask) -> Result<(), CodecError> {
    super::write_delegation(writer, value.assignment())?;
    writer.write_u8(crate::canonical::task_phase_tag(value.phase()))?;
    writer.write_option_tag(value.reservation().is_some())?;
    if let Some(reservation) = value.reservation() {
        super::write_reservation(writer, reservation)?;
    }
    writer.write_option_tag(value.terminal().is_some())?;
    if let Some(terminal) = value.terminal() {
        super::write_task_terminal(writer, terminal)?;
    }
    Ok(())
}

fn read_task(reader: &mut CanonicalReader<'_>) -> Result<CollaborationTask, CodecError> {
    let assignment = super::read_delegation(reader)?;
    let offset = reader.offset();
    let phase = match reader.read_u8()? {
        1 => TaskPhase::Offered,
        2 => TaskPhase::Accepted,
        3 => TaskPhase::Active,
        4 => TaskPhase::Cancelling,
        5 => TaskPhase::Terminal,
        _ => return Err(super::unknown(offset)),
    };
    let reservation =
        reader.read_option_tag()?.then(|| super::read_reservation(reader)).transpose()?;
    let terminal =
        reader.read_option_tag()?.then(|| super::read_task_terminal(reader)).transpose()?;
    Ok(CollaborationTask::from_wire(assignment, phase, reservation, terminal))
}

fn write_terminal(
    writer: &mut CanonicalWriter,
    value: &CollaborationTerminal,
) -> Result<(), CodecError> {
    writer.write_u8(crate::canonical::collaboration_terminal_tag(value.kind()))?;
    writer.write_collection_len(value.blocking_tasks().len())?;
    for task in value.blocking_tasks() {
        super::write_id(writer, task.as_bytes())?;
    }
    super::write_digest(writer, value.digest())
}

fn read_terminal(reader: &mut CanonicalReader<'_>) -> Result<CollaborationTerminal, CodecError> {
    let offset = reader.offset();
    let kind = match reader.read_u8()? {
        1 => CollaborationTerminalKind::Completed,
        2 => CollaborationTerminalKind::Failed,
        3 => CollaborationTerminalKind::Cancelled,
        4 => CollaborationTerminalKind::Abandoned,
        5 => CollaborationTerminalKind::UnsatisfiedJoin,
        _ => return Err(super::unknown(offset)),
    };
    let count = super::bounded_len(reader, crate::CollaborationLimits::MAX_TASKS as usize)?;
    let mut blocking = Vec::with_capacity(count);
    for _ in 0..count {
        blocking.push(super::read_task_id(reader)?);
    }
    if blocking.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::invalid(reader));
    }
    Ok(CollaborationTerminal::from_wire(kind, blocking, super::read_digest(reader)?))
}
