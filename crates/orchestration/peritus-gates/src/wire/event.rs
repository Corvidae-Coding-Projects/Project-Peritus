use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::EventSequence;

use crate::{GateEvent, GateEventKind, RecoveryDisposition};

pub struct GateEventFrame(pub GateEvent);

impl GateEventFrame {
    pub fn into_event(self) -> GateEvent {
        self.0
    }
}

impl CanonicalEncode for GateEventFrame {
    const FAMILY: u16 = 51;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let event = &self.0;
        super::write_id(writer, event.id().as_bytes())?;
        super::write_id(writer, event.command_id().as_bytes())?;
        writer.write_u64(event.sequence().get())?;
        super::write_option_id(writer, event.previous_event())?;
        super::write_id(writer, event.run_id().as_bytes())?;
        super::write_revision(writer, event.revision())?;
        super::write_digest(writer, event.prior_state_digest())?;
        super::write_digest(writer, event.successor_state_digest())?;
        write_kind(writer, event.kind())
    }
}

impl CanonicalDecode for GateEventFrame {
    const FAMILY: u16 = 51;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = super::read_event_id(reader)?;
        let command_id = super::read_command_id(reader)?;
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let previous = super::read_option_event(reader)?;
        if (sequence.get() == 1) != previous.is_none() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset));
        }
        Ok(Self(GateEvent::new(
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

fn write_kind(writer: &mut CanonicalWriter, kind: &GateEventKind) -> Result<(), CodecError> {
    match kind {
        GateEventKind::RunStarted { snapshot_digest } => {
            writer.write_u8(1)?;
            super::write_digest(writer, *snapshot_digest)?;
        }
        GateEventKind::AttemptPrepared { gate_id, attempt } => {
            writer.write_u8(2)?;
            super::write_id(writer, gate_id.as_bytes())?;
            super::write_attempt(writer, *attempt)?;
        }
        GateEventKind::AttemptDispatched { gate_id, execution_id } => {
            writer.write_u8(3)?;
            super::write_id(writer, gate_id.as_bytes())?;
            super::write_id(writer, execution_id.as_bytes())?;
        }
        GateEventKind::ResultObserved { gate_id, execution_id, result } => {
            writer.write_u8(4)?;
            super::write_id(writer, gate_id.as_bytes())?;
            super::write_id(writer, execution_id.as_bytes())?;
            super::write_result(writer, result)?;
        }
        GateEventKind::RecoveryClassified { gate_id, execution_id, disposition } => {
            writer.write_u8(5)?;
            super::write_id(writer, gate_id.as_bytes())?;
            super::write_id(writer, execution_id.as_bytes())?;
            writer.write_u8(recovery_tag(*disposition))?;
        }
        GateEventKind::EvidencePublished { gate_id, execution_id, receipt } => {
            writer.write_u8(6)?;
            super::write_id(writer, gate_id.as_bytes())?;
            super::write_id(writer, execution_id.as_bytes())?;
            super::write_receipt(writer, receipt)?;
        }
        GateEventKind::CancellationStarted => writer.write_u8(7)?,
        GateEventKind::RunFinalized => writer.write_u8(8)?,
    }
    Ok(())
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<GateEventKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(GateEventKind::RunStarted { snapshot_digest: super::read_digest(reader)? }),
        2 => Ok(GateEventKind::AttemptPrepared {
            gate_id: super::read_gate_id(reader)?,
            attempt: super::read_attempt(reader)?,
        }),
        3 => Ok(GateEventKind::AttemptDispatched {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
        }),
        4 => Ok(GateEventKind::ResultObserved {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
            result: super::read_result(reader)?,
        }),
        5 => Ok(GateEventKind::RecoveryClassified {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
            disposition: read_recovery(reader)?,
        }),
        6 => Ok(GateEventKind::EvidencePublished {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
            receipt: super::read_receipt(reader)?,
        }),
        7 => Ok(GateEventKind::CancellationStarted),
        8 => Ok(GateEventKind::RunFinalized),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn recovery_tag(value: RecoveryDisposition) -> u8 {
    match value {
        RecoveryDisposition::SafeToRetry => 1,
        RecoveryDisposition::TerminalFailure => 2,
        RecoveryDisposition::StillActive => 3,
    }
}

fn read_recovery(reader: &mut CanonicalReader<'_>) -> Result<RecoveryDisposition, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(RecoveryDisposition::SafeToRetry),
        2 => Ok(RecoveryDisposition::TerminalFailure),
        3 => Ok(RecoveryDisposition::StillActive),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
