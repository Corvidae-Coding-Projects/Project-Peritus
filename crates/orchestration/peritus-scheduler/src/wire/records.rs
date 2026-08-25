//! Wire encoding for mutable records and terminal summaries.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};

use crate::{
    SchedulerLimits, SchedulerPhase, SchedulerTerminal, SchedulerTerminalKind, WorkPhase,
    WorkRecord, WorkTerminal, WorkerPhase, WorkerRecord,
};

pub fn write_worker_record(
    writer: &mut CanonicalWriter,
    value: &WorkerRecord,
) -> Result<(), CodecError> {
    super::domain::write_descriptor(writer, value.descriptor())?;
    writer.write_u8(worker_phase_tag(value.phase()))
}

pub fn read_worker_record(
    reader: &mut CanonicalReader<'_>,
    limits: SchedulerLimits,
) -> Result<WorkerRecord, CodecError> {
    let descriptor = super::domain::read_descriptor(reader, limits)?;
    let offset = reader.offset();
    let phase = match reader.read_u8()? {
        1 => WorkerPhase::Available,
        2 => WorkerPhase::Busy,
        3 => WorkerPhase::Draining,
        4 => WorkerPhase::Lost,
        5 => WorkerPhase::Removed,
        _ => return Err(super::unknown(offset)),
    };
    Ok(WorkerRecord::from_wire(descriptor, phase))
}

pub fn write_work_record(
    writer: &mut CanonicalWriter,
    value: &WorkRecord,
) -> Result<(), CodecError> {
    super::domain::write_spec(writer, value.spec())?;
    writer.write_u8(work_phase_tag(value.phase()))?;
    writer.write_u64(value.enqueue_ordinal())?;
    writer.write_u16(value.bypasses())?;
    writer.write_u16(value.attempts_started())?;
    writer.write_option_tag(value.retry_cause().is_some())?;
    if let Some(digest) = value.retry_cause() {
        super::write_digest(writer, digest)?;
    }
    writer.write_option_tag(value.terminal().is_some())?;
    if let Some(terminal) = value.terminal() {
        write_work_terminal(writer, terminal)?;
    }
    Ok(())
}

pub fn read_work_record(
    reader: &mut CanonicalReader<'_>,
    limits: SchedulerLimits,
) -> Result<WorkRecord, CodecError> {
    let spec = super::domain::read_spec(reader, limits)?;
    let offset = reader.offset();
    let phase = match reader.read_u8()? {
        1 => WorkPhase::WaitingDependencies,
        2 => WorkPhase::Queued,
        3 => WorkPhase::Reserved,
        4 => WorkPhase::Running,
        5 => WorkPhase::RetryPending,
        6 => WorkPhase::Cancelling,
        7 => WorkPhase::Terminal,
        _ => return Err(super::unknown(offset)),
    };
    let enqueue = reader.read_u64()?;
    let bypasses = reader.read_u16()?;
    let attempts = reader.read_u16()?;
    let retry = reader.read_option_tag()?.then(|| super::read_digest(reader)).transpose()?;
    let terminal = reader.read_option_tag()?.then(|| read_work_terminal(reader)).transpose()?;
    Ok(WorkRecord::from_wire(spec, phase, enqueue, bypasses, attempts, retry, terminal))
}

fn write_work_terminal(
    writer: &mut CanonicalWriter,
    value: &WorkTerminal,
) -> Result<(), CodecError> {
    match value {
        WorkTerminal::Succeeded { result_digest } => {
            writer.write_u8(1)?;
            super::write_digest(writer, *result_digest)
        }
        WorkTerminal::Failed { failure_digest } => {
            writer.write_u8(2)?;
            super::write_digest(writer, *failure_digest)
        }
        WorkTerminal::DependencyFailed { dependency } => {
            writer.write_u8(3)?;
            super::write_id(writer, dependency.as_bytes())
        }
        WorkTerminal::Cancelled => writer.write_u8(4),
        WorkTerminal::Ambiguous { dispatch_id } => {
            writer.write_u8(5)?;
            super::write_id(writer, dispatch_id.as_bytes())
        }
        WorkTerminal::Exhausted { cause_digest } => {
            writer.write_u8(6)?;
            super::write_digest(writer, *cause_digest)
        }
        WorkTerminal::Abandoned { cause_digest } => {
            writer.write_u8(7)?;
            super::write_digest(writer, *cause_digest)
        }
    }
}

fn read_work_terminal(reader: &mut CanonicalReader<'_>) -> Result<WorkTerminal, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(WorkTerminal::Succeeded { result_digest: super::read_digest(reader)? }),
        2 => Ok(WorkTerminal::Failed { failure_digest: super::read_digest(reader)? }),
        3 => Ok(WorkTerminal::DependencyFailed { dependency: super::read_work_id(reader)? }),
        4 => Ok(WorkTerminal::Cancelled),
        5 => Ok(WorkTerminal::Ambiguous { dispatch_id: super::read_dispatch_id(reader)? }),
        6 => Ok(WorkTerminal::Exhausted { cause_digest: super::read_digest(reader)? }),
        7 => Ok(WorkTerminal::Abandoned { cause_digest: super::read_digest(reader)? }),
        _ => Err(super::unknown(offset)),
    }
}

pub fn write_terminal(
    writer: &mut CanonicalWriter,
    value: &SchedulerTerminal,
) -> Result<(), CodecError> {
    writer.write_u8(terminal_kind_tag(value.kind()))?;
    writer.write_collection_len(value.non_successful_work().len())?;
    for work in value.non_successful_work() {
        super::write_id(writer, work.as_bytes())?;
    }
    super::write_digest(writer, value.digest())
}

pub fn read_terminal(reader: &mut CanonicalReader<'_>) -> Result<SchedulerTerminal, CodecError> {
    let offset = reader.offset();
    let kind = match reader.read_u8()? {
        1 => SchedulerTerminalKind::Completed,
        2 => SchedulerTerminalKind::Failed,
        3 => SchedulerTerminalKind::DependencyFailed,
        4 => SchedulerTerminalKind::Ambiguous,
        5 => SchedulerTerminalKind::Exhausted,
        6 => SchedulerTerminalKind::Cancelled,
        _ => return Err(super::unknown(offset)),
    };
    let count = reader.read_collection_len()?;
    let mut work = Vec::with_capacity(count);
    for _ in 0..count {
        work.push(super::read_work_id(reader)?);
    }
    if work.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::invalid(reader));
    }
    Ok(SchedulerTerminal::from_wire(kind, work, super::read_digest(reader)?))
}

const fn worker_phase_tag(value: WorkerPhase) -> u8 {
    match value {
        WorkerPhase::Available => 1,
        WorkerPhase::Busy => 2,
        WorkerPhase::Draining => 3,
        WorkerPhase::Lost => 4,
        WorkerPhase::Removed => 5,
    }
}

const fn work_phase_tag(value: WorkPhase) -> u8 {
    match value {
        WorkPhase::WaitingDependencies => 1,
        WorkPhase::Queued => 2,
        WorkPhase::Reserved => 3,
        WorkPhase::Running => 4,
        WorkPhase::RetryPending => 5,
        WorkPhase::Cancelling => 6,
        WorkPhase::Terminal => 7,
    }
}

pub const fn scheduler_phase_tag(value: SchedulerPhase) -> u8 {
    match value {
        SchedulerPhase::Active => 1,
        SchedulerPhase::Paused => 2,
        SchedulerPhase::Draining => 3,
        SchedulerPhase::DrainingPaused => 4,
        SchedulerPhase::Terminal => 5,
    }
}

const fn terminal_kind_tag(value: SchedulerTerminalKind) -> u8 {
    match value {
        SchedulerTerminalKind::Completed => 1,
        SchedulerTerminalKind::Failed => 2,
        SchedulerTerminalKind::DependencyFailed => 3,
        SchedulerTerminalKind::Ambiguous => 4,
        SchedulerTerminalKind::Exhausted => 5,
        SchedulerTerminalKind::Cancelled => 6,
    }
}
