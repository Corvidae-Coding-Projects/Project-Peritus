use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use crate::{FailureDisposition, SchedulerCommand, SchedulerCommandKind};

/// Canonical family-70 schema-v1 scheduler command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerCommandFrame(SchedulerCommand);

impl SchedulerCommandFrame {
    /// Clones a command into an inert canonical frame.
    #[must_use]
    pub fn from_command(command: &SchedulerCommand) -> Self {
        Self(command.clone())
    }
    /// Consumes the frame.
    #[must_use]
    pub fn into_command(self) -> SchedulerCommand {
        self.0
    }
}

impl CanonicalEncode for SchedulerCommandFrame {
    const FAMILY: u16 = 70;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let command = &self.0;
        super::write_id(writer, command.command_id().as_bytes())?;
        super::write_id(writer, command.event_id().as_bytes())?;
        super::write_id(writer, command.run_id().as_bytes())?;
        writer.write_u64(command.expected_sequence())?;
        super::write_option_id(
            writer,
            command.expected_previous_event(),
            peritus_types::EventId::into_bytes,
        )?;
        super::write_digest(writer, command.prior_state_digest())?;
        super::write_revision(writer, command.revision())?;
        write_kind(writer, command.kind())
    }
}

impl CanonicalDecode for SchedulerCommandFrame {
    const FAMILY: u16 = 70;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::read_command_id(reader)?;
        let event_id = super::read_event_id(reader)?;
        let run_id = super::read_run_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let previous =
            reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
        if (expected_sequence == 0) != previous.is_none() {
            return Err(super::invalid(reader));
        }
        let prior = super::read_digest(reader)?;
        let revision = super::read_revision(reader)?;
        SchedulerCommand::new(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            previous,
            prior,
            revision,
            read_kind(reader)?,
        )
        .map(Self)
        .map_err(|_| super::invalid(reader))
    }
}

fn write_kind(writer: &mut CanonicalWriter, kind: &SchedulerCommandKind) -> Result<(), CodecError> {
    match kind {
        SchedulerCommandKind::StartScheduler { binding } => {
            writer.write_u8(1)?;
            super::write_binding(writer, binding)
        }
        SchedulerCommandKind::RegisterWorker { descriptor } => {
            writer.write_u8(2)?;
            super::write_descriptor(writer, descriptor)
        }
        SchedulerCommandKind::SetWorkerAvailable { worker_id } => {
            writer.write_u8(3)?;
            super::write_id(writer, worker_id.as_bytes())
        }
        SchedulerCommandKind::DrainWorker { worker_id } => {
            writer.write_u8(4)?;
            super::write_id(writer, worker_id.as_bytes())
        }
        SchedulerCommandKind::LoseWorker { worker_id } => {
            writer.write_u8(5)?;
            super::write_id(writer, worker_id.as_bytes())
        }
        SchedulerCommandKind::RemoveWorker { worker_id } => {
            writer.write_u8(6)?;
            super::write_id(writer, worker_id.as_bytes())
        }
        SchedulerCommandKind::AdmitWork { spec } => {
            writer.write_u8(7)?;
            super::write_spec(writer, spec)
        }
        SchedulerCommandKind::DispatchNext { dispatch_id, dispatch_token } => {
            writer.write_u8(8)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *dispatch_token)
        }
        SchedulerCommandKind::AcknowledgeStart { dispatch_id } => {
            writer.write_u8(9)?;
            super::write_id(writer, dispatch_id.as_bytes())
        }
        SchedulerCommandKind::CompleteWork { dispatch_id, result_digest } => {
            writer.write_u8(10)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *result_digest)
        }
        SchedulerCommandKind::FailWork { dispatch_id, failure_digest, disposition } => {
            writer.write_u8(11)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *failure_digest)?;
            writer.write_u8(super::failure_disposition_tag(*disposition))
        }
        SchedulerCommandKind::RetryWork { work_id } => {
            writer.write_u8(12)?;
            super::write_id(writer, work_id.as_bytes())
        }
        SchedulerCommandKind::CancelWork { work_id } => {
            writer.write_u8(13)?;
            super::write_id(writer, work_id.as_bytes())
        }
        SchedulerCommandKind::CancelWorkTree { work_id } => {
            writer.write_u8(14)?;
            super::write_id(writer, work_id.as_bytes())
        }
        SchedulerCommandKind::AcknowledgeCancellation { dispatch_id } => {
            writer.write_u8(15)?;
            super::write_id(writer, dispatch_id.as_bytes())
        }
        SchedulerCommandKind::ExhaustWork { work_id, cause_digest } => {
            writer.write_u8(16)?;
            super::write_id(writer, work_id.as_bytes())?;
            super::write_digest(writer, *cause_digest)
        }
        SchedulerCommandKind::AbandonDispatch { dispatch_id, cause_digest } => {
            writer.write_u8(17)?;
            super::write_id(writer, dispatch_id.as_bytes())?;
            super::write_digest(writer, *cause_digest)
        }
        SchedulerCommandKind::PauseScheduler => writer.write_u8(18),
        SchedulerCommandKind::ResumeScheduler => writer.write_u8(19),
        SchedulerCommandKind::DrainScheduler => writer.write_u8(20),
        SchedulerCommandKind::FinalizeScheduler => writer.write_u8(21),
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<SchedulerCommandKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(SchedulerCommandKind::StartScheduler { binding: super::read_binding(reader)? }),
        2 => Ok(SchedulerCommandKind::RegisterWorker {
            descriptor: super::read_descriptor(reader, super::production_limits())?,
        }),
        3 => Ok(SchedulerCommandKind::SetWorkerAvailable {
            worker_id: super::read_worker_id(reader)?,
        }),
        4 => Ok(SchedulerCommandKind::DrainWorker { worker_id: super::read_worker_id(reader)? }),
        5 => Ok(SchedulerCommandKind::LoseWorker { worker_id: super::read_worker_id(reader)? }),
        6 => Ok(SchedulerCommandKind::RemoveWorker { worker_id: super::read_worker_id(reader)? }),
        7 => Ok(SchedulerCommandKind::AdmitWork {
            spec: super::read_spec(reader, super::production_limits())?,
        }),
        8 => Ok(SchedulerCommandKind::DispatchNext {
            dispatch_id: super::read_dispatch_id(reader)?,
            dispatch_token: super::read_digest(reader)?,
        }),
        9 => Ok(SchedulerCommandKind::AcknowledgeStart {
            dispatch_id: super::read_dispatch_id(reader)?,
        }),
        10 => Ok(SchedulerCommandKind::CompleteWork {
            dispatch_id: super::read_dispatch_id(reader)?,
            result_digest: super::read_digest(reader)?,
        }),
        11 => Ok(SchedulerCommandKind::FailWork {
            dispatch_id: super::read_dispatch_id(reader)?,
            failure_digest: super::read_digest(reader)?,
            disposition: read_disposition(reader)?,
        }),
        12 => Ok(SchedulerCommandKind::RetryWork { work_id: super::read_work_id(reader)? }),
        13 => Ok(SchedulerCommandKind::CancelWork { work_id: super::read_work_id(reader)? }),
        14 => Ok(SchedulerCommandKind::CancelWorkTree { work_id: super::read_work_id(reader)? }),
        15 => Ok(SchedulerCommandKind::AcknowledgeCancellation {
            dispatch_id: super::read_dispatch_id(reader)?,
        }),
        16 => Ok(SchedulerCommandKind::ExhaustWork {
            work_id: super::read_work_id(reader)?,
            cause_digest: super::read_digest(reader)?,
        }),
        17 => Ok(SchedulerCommandKind::AbandonDispatch {
            dispatch_id: super::read_dispatch_id(reader)?,
            cause_digest: super::read_digest(reader)?,
        }),
        18 => Ok(SchedulerCommandKind::PauseScheduler),
        19 => Ok(SchedulerCommandKind::ResumeScheduler),
        20 => Ok(SchedulerCommandKind::DrainScheduler),
        21 => Ok(SchedulerCommandKind::FinalizeScheduler),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

pub fn read_disposition(
    reader: &mut CanonicalReader<'_>,
) -> Result<FailureDisposition, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(FailureDisposition::Retryable),
        2 => Ok(FailureDisposition::Failed),
        3 => Ok(FailureDisposition::Ambiguous),
        _ => Err(super::unknown(offset)),
    }
}
