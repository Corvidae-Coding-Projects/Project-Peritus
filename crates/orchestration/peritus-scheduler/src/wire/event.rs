use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::EventSequence;

use crate::{LossOutcome, SchedulerEvent, SchedulerEventKind};

/// Canonical family-71 schema-v1 scheduler event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerEventFrame(SchedulerEvent);

impl SchedulerEventFrame {
    /// Wraps one immutable event.
    #[must_use]
    pub const fn new(event: SchedulerEvent) -> Self {
        Self(event)
    }
    /// Consumes the frame.
    #[must_use]
    pub fn into_event(self) -> SchedulerEvent {
        self.0
    }
}

impl CanonicalEncode for SchedulerEventFrame {
    const FAMILY: u16 = 71;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let event = &self.0;
        super::write_id(writer, event.id().as_bytes())?;
        super::write_id(writer, event.command_id().as_bytes())?;
        writer.write_u64(event.sequence().get())?;
        super::write_option_id(writer, event.previous_event(), peritus_types::EventId::into_bytes)?;
        super::write_id(writer, event.run_id().as_bytes())?;
        super::write_revision(writer, event.revision())?;
        super::write_digest(writer, event.prior_state_digest())?;
        super::write_digest(writer, event.successor_state_digest())?;
        write_kind(writer, event.kind())
    }
}

impl CanonicalDecode for SchedulerEventFrame {
    const FAMILY: u16 = 71;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = super::read_event_id(reader)?;
        let command = super::read_command_id(reader)?;
        let offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
        let previous =
            reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
        if (sequence.get() == 1) != previous.is_none() {
            return Err(super::invalid(reader));
        }
        Ok(Self(SchedulerEvent::from_wire(
            id,
            command,
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

fn write_kind(writer: &mut CanonicalWriter, kind: &SchedulerEventKind) -> Result<(), CodecError> {
    match kind {
        SchedulerEventKind::SchedulerStarted { binding } => {
            writer.write_u8(1)?;
            super::write_binding(writer, binding)
        }
        SchedulerEventKind::WorkerRegistered { descriptor } => {
            writer.write_u8(2)?;
            super::write_descriptor(writer, descriptor)
        }
        SchedulerEventKind::WorkerAvailable { worker_id } => {
            id_kind(writer, 3, worker_id.as_bytes())
        }
        SchedulerEventKind::WorkerDrainRequested { worker_id } => {
            id_kind(writer, 4, worker_id.as_bytes())
        }
        SchedulerEventKind::WorkerLost { worker_id, outcomes } => {
            writer.write_u8(5)?;
            super::write_id(writer, worker_id.as_bytes())?;
            writer.write_collection_len(outcomes.len())?;
            for outcome in outcomes {
                write_loss(writer, outcome)?;
            }
            Ok(())
        }
        SchedulerEventKind::WorkerRemoved { worker_id } => id_kind(writer, 6, worker_id.as_bytes()),
        SchedulerEventKind::WorkAdmitted { spec } => {
            writer.write_u8(7)?;
            super::write_spec(writer, spec)
        }
        SchedulerEventKind::WorkReserved { reservation } => {
            writer.write_u8(8)?;
            super::write_reservation(writer, reservation)
        }
        SchedulerEventKind::WorkStartAcknowledged { dispatch_id } => {
            id_kind(writer, 9, dispatch_id.as_bytes())
        }
        SchedulerEventKind::WorkSucceeded { dispatch_id, result_digest } => {
            writer.write_u8(10)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *result_digest)
        }
        SchedulerEventKind::WorkFailed { dispatch_id, failure_digest, disposition } => {
            writer.write_u8(11)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *failure_digest)?;
            writer.write_u8(super::failure_disposition_tag(*disposition))
        }
        SchedulerEventKind::WorkRetryQueued { work_id } => id_kind(writer, 12, work_id.as_bytes()),
        SchedulerEventKind::WorkCancelled { work_id, descendants, affected } => {
            writer.write_u8(13)?;
            super::write_id(writer, work_id.as_bytes())?;
            writer.write_bool(*descendants)?;
            writer.write_collection_len(affected.len())?;
            for id in affected {
                super::write_id(writer, id.as_bytes())?;
            }
            Ok(())
        }
        SchedulerEventKind::CancellationAcknowledged { dispatch_id } => {
            id_kind(writer, 14, dispatch_id.as_bytes())
        }
        SchedulerEventKind::WorkExhausted { work_id, cause_digest } => {
            writer.write_u8(15)?;
            super::write_id(writer, work_id.as_bytes())?;
            super::write_digest(writer, *cause_digest)
        }
        SchedulerEventKind::DispatchAbandoned { dispatch_id, cause_digest } => {
            writer.write_u8(16)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *cause_digest)
        }
        SchedulerEventKind::SchedulerPaused => writer.write_u8(17),
        SchedulerEventKind::SchedulerResumed => writer.write_u8(18),
        SchedulerEventKind::SchedulerDrainRequested => writer.write_u8(19),
        SchedulerEventKind::SchedulerFinalized { terminal } => {
            writer.write_u8(20)?;
            super::write_terminal(writer, terminal)
        }
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<SchedulerEventKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(SchedulerEventKind::SchedulerStarted { binding: super::read_binding(reader)? }),
        2 => Ok(SchedulerEventKind::WorkerRegistered {
            descriptor: super::read_descriptor(reader, super::production_limits())?,
        }),
        3 => Ok(SchedulerEventKind::WorkerAvailable { worker_id: super::read_worker_id(reader)? }),
        4 => Ok(SchedulerEventKind::WorkerDrainRequested {
            worker_id: super::read_worker_id(reader)?,
        }),
        5 => read_worker_loss(reader),
        6 => Ok(SchedulerEventKind::WorkerRemoved { worker_id: super::read_worker_id(reader)? }),
        7 => Ok(SchedulerEventKind::WorkAdmitted {
            spec: super::read_spec(reader, super::production_limits())?,
        }),
        8 => Ok(SchedulerEventKind::WorkReserved {
            reservation: super::read_reservation(reader, super::production_limits())?,
        }),
        9 => Ok(SchedulerEventKind::WorkStartAcknowledged {
            dispatch_id: super::read_dispatch_id(reader)?,
        }),
        10 => Ok(SchedulerEventKind::WorkSucceeded {
            dispatch_id: super::read_dispatch_id(reader)?,
            result_digest: super::read_digest(reader)?,
        }),
        11 => Ok(SchedulerEventKind::WorkFailed {
            dispatch_id: super::read_dispatch_id(reader)?,
            failure_digest: super::read_digest(reader)?,
            disposition: super::command::read_disposition(reader)?,
        }),
        12 => Ok(SchedulerEventKind::WorkRetryQueued { work_id: super::read_work_id(reader)? }),
        13 => read_cancelled(reader),
        14 => Ok(SchedulerEventKind::CancellationAcknowledged {
            dispatch_id: super::read_dispatch_id(reader)?,
        }),
        15 => Ok(SchedulerEventKind::WorkExhausted {
            work_id: super::read_work_id(reader)?,
            cause_digest: super::read_digest(reader)?,
        }),
        16 => Ok(SchedulerEventKind::DispatchAbandoned {
            dispatch_id: super::read_dispatch_id(reader)?,
            cause_digest: super::read_digest(reader)?,
        }),
        17 => Ok(SchedulerEventKind::SchedulerPaused),
        18 => Ok(SchedulerEventKind::SchedulerResumed),
        19 => Ok(SchedulerEventKind::SchedulerDrainRequested),
        20 => {
            Ok(SchedulerEventKind::SchedulerFinalized { terminal: super::read_terminal(reader)? })
        }
        _ => Err(super::unknown(offset)),
    }
}

fn id_kind(writer: &mut CanonicalWriter, tag: u8, bytes: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_u8(tag)?;
    super::write_id(writer, bytes)
}
fn write_loss(writer: &mut CanonicalWriter, value: &LossOutcome) -> Result<(), CodecError> {
    let (tag, dispatch, work) = match value {
        LossOutcome::Requeued { dispatch_id, work_id } => (1, dispatch_id, work_id),
        LossOutcome::Exhausted { dispatch_id, work_id } => (2, dispatch_id, work_id),
        LossOutcome::Ambiguous { dispatch_id, work_id } => (3, dispatch_id, work_id),
        LossOutcome::Failed { dispatch_id, work_id } => (4, dispatch_id, work_id),
        LossOutcome::Cancelled { dispatch_id, work_id } => (5, dispatch_id, work_id),
    };
    writer.write_u8(tag)?;
    super::write_id(writer, dispatch.as_bytes())?;
    super::write_id(writer, work.as_bytes())
}
fn read_worker_loss(reader: &mut CanonicalReader<'_>) -> Result<SchedulerEventKind, CodecError> {
    let worker_id = super::read_worker_id(reader)?;
    let count = reader.read_collection_len()?;
    if count > usize::from(crate::SchedulerLimits::MAX_ACTIVE_RESERVATIONS) {
        return Err(super::invalid(reader));
    }
    let mut outcomes = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = reader.offset();
        let tag = reader.read_u8()?;
        let dispatch_id = super::read_dispatch_id(reader)?;
        let work_id = super::read_work_id(reader)?;
        outcomes.push(match tag {
            1 => LossOutcome::Requeued { dispatch_id, work_id },
            2 => LossOutcome::Exhausted { dispatch_id, work_id },
            3 => LossOutcome::Ambiguous { dispatch_id, work_id },
            4 => LossOutcome::Failed { dispatch_id, work_id },
            5 => LossOutcome::Cancelled { dispatch_id, work_id },
            _ => return Err(super::unknown(offset)),
        });
    }
    Ok(SchedulerEventKind::WorkerLost { worker_id, outcomes })
}
fn read_cancelled(reader: &mut CanonicalReader<'_>) -> Result<SchedulerEventKind, CodecError> {
    let work_id = super::read_work_id(reader)?;
    let descendants = reader.read_bool()?;
    let count = reader.read_collection_len()?;
    if count > crate::SchedulerLimits::MAX_RETAINED_WORK as usize {
        return Err(super::invalid(reader));
    }
    let mut affected = Vec::with_capacity(count);
    for _ in 0..count {
        affected.push(super::read_work_id(reader)?);
    }
    if affected.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::invalid(reader));
    }
    Ok(SchedulerEventKind::WorkCancelled { work_id, descendants, affected })
}
