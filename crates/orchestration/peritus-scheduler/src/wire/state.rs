use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::EventSequence;

use crate::{SchedulerPhase, SchedulerState};

/// Canonical family-72 schema-v1 complete scheduler checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerStateFrame(SchedulerState);

impl SchedulerStateFrame {
    /// Clones complete state into an inert frame.
    #[must_use]
    pub fn from_state(state: &SchedulerState) -> Self {
        Self(state.clone())
    }
    /// Returns whether every decoded field equals authoritative state.
    #[must_use]
    pub fn matches_state(&self, state: &SchedulerState) -> bool {
        &self.0 == state
    }
    /// Returns run identity.
    #[must_use]
    pub const fn run_id(&self) -> peritus_types::RunId {
        self.0.run_id()
    }
    /// Returns sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.0.sequence()
    }
    /// Returns last event.
    #[must_use]
    pub const fn last_event_id(&self) -> peritus_types::EventId {
        self.0.last_event_id()
    }
    /// Returns exact revision.
    #[must_use]
    pub const fn revision(&self) -> peritus_types::RevisionTuple {
        self.0.binding().revision()
    }
    /// Returns complete state digest.
    #[must_use]
    pub const fn state_digest(&self) -> peritus_types::Sha256Digest {
        self.0.state_digest()
    }
    /// Consumes this checked checkpoint.
    #[must_use]
    pub fn into_state(self) -> SchedulerState {
        self.0
    }
}

impl CanonicalEncode for SchedulerStateFrame {
    const FAMILY: u16 = 72;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let state = &self.0;
        super::write_binding(writer, state.binding())?;
        writer.write_u8(super::scheduler_phase_tag(state.phase()))?;
        writer.write_u64(state.sequence().get())?;
        super::write_id(writer, state.last_event_id().as_bytes())?;
        super::write_digest(writer, state.state_digest())?;
        writer.write_collection_len(state.workers().len())?;
        for worker in state.workers() {
            super::write_worker_record(writer, worker)?;
        }
        writer.write_collection_len(state.work().len())?;
        for work in state.work() {
            super::write_work_record(writer, work)?;
        }
        writer.write_collection_len(state.reservations().len())?;
        for reservation in state.reservations() {
            super::write_reservation(writer, reservation)?;
        }
        writer.write_collection_len(state.used_dispatches().len())?;
        for dispatch in state.used_dispatches() {
            super::write_id(writer, dispatch.as_bytes())?;
        }
        writer.write_u64(state.enqueue_ordinal())?;
        writer.write_u64(state.dispatch_ordinal())?;
        writer.write_collection_len(state.used_commands().len())?;
        for command in state.used_commands() {
            super::write_id(writer, command.as_bytes())?;
        }
        writer.write_option_tag(state.terminal().is_some())?;
        if let Some(terminal) = state.terminal() {
            super::write_terminal(writer, terminal)?;
        }
        Ok(())
    }
}

impl CanonicalDecode for SchedulerStateFrame {
    const FAMILY: u16 = 72;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let binding = super::read_binding(reader)?;
        let limits = binding.limits();
        let offset = reader.offset();
        let phase = match reader.read_u8()? {
            1 => SchedulerPhase::Active,
            2 => SchedulerPhase::Paused,
            3 => SchedulerPhase::Draining,
            4 => SchedulerPhase::DrainingPaused,
            5 => SchedulerPhase::Terminal,
            _ => return Err(super::unknown(offset)),
        };
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let event = super::read_event_id(reader)?;
        let digest = super::read_digest(reader)?;
        let worker_count = bounded(reader, usize::from(limits.workers()))?;
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            workers.push(super::read_worker_record(reader, limits)?);
        }
        let work_count = bounded(reader, limits.retained_work() as usize)?;
        let mut work = Vec::with_capacity(work_count);
        for _ in 0..work_count {
            work.push(super::read_work_record(reader, limits)?);
        }
        let reservation_count = bounded(reader, usize::from(limits.active_reservations()))?;
        let mut reservations = Vec::with_capacity(reservation_count);
        for _ in 0..reservation_count {
            reservations.push(super::read_reservation(reader, limits)?);
        }
        let dispatch_count = bounded(
            reader,
            (limits.retained_work() as usize)
                .saturating_mul(usize::from(limits.attempts_per_work())),
        )?;
        let mut dispatches = Vec::with_capacity(dispatch_count);
        for _ in 0..dispatch_count {
            dispatches.push(super::read_dispatch_id(reader)?);
        }
        let enqueue = reader.read_u64()?;
        let dispatch = reader.read_u64()?;
        let command_count = reader.read_collection_len()?;
        let mut commands = Vec::with_capacity(command_count);
        for _ in 0..command_count {
            commands.push(super::read_command_id(reader)?);
        }
        let terminal =
            reader.read_option_tag()?.then(|| super::read_terminal(reader)).transpose()?;
        let state = SchedulerState::from_wire(
            binding,
            phase,
            sequence,
            event,
            digest,
            workers,
            work,
            reservations,
            dispatches,
            enqueue,
            dispatch,
            commands,
            terminal,
        );
        state.validate_inert().map_err(|_| super::invalid(reader))?;
        Ok(Self(state))
    }
}

fn bounded(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count > maximum { Err(super::invalid(reader)) } else { Ok(count) }
}
