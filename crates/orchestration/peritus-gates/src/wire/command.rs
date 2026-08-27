use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use crate::{GateCommand, GateCommandKind};

/// Canonical B3 command frame for the D1 gate aggregate.
pub struct GateCommandFrame(GateCommand);

impl GateCommandFrame {
    /// Wraps one syntax-checked command for canonical transport.
    #[must_use]
    pub fn from_command(command: &GateCommand) -> Self {
        Self(command.clone())
    }

    /// Consumes the frame into its inert command.
    #[must_use]
    pub fn into_command(self) -> GateCommand {
        self.0
    }
}

impl CanonicalEncode for GateCommandFrame {
    const FAMILY: u16 = 50;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let command = &self.0;
        super::write_id(writer, command.command_id().as_bytes())?;
        super::write_id(writer, command.event_id().as_bytes())?;
        super::write_id(writer, command.run_id().as_bytes())?;
        writer.write_u64(command.expected_sequence())?;
        super::write_option_id(writer, command.expected_previous_event())?;
        super::write_digest(writer, command.prior_state_digest())?;
        super::write_revision(writer, command.revision())?;
        match command.kind() {
            GateCommandKind::StartRun { snapshot_digest } => {
                writer.write_u8(1)?;
                super::write_digest(writer, *snapshot_digest)?;
            }
            GateCommandKind::PrepareAttempt { gate_id, attempt } => {
                writer.write_u8(2)?;
                super::write_id(writer, gate_id.as_bytes())?;
                super::write_attempt(writer, *attempt)?;
            }
            GateCommandKind::MarkDispatched { gate_id, execution_id } => {
                writer.write_u8(3)?;
                super::write_id(writer, gate_id.as_bytes())?;
                super::write_id(writer, execution_id.as_bytes())?;
            }
            GateCommandKind::ObserveResult { gate_id, execution_id, result } => {
                writer.write_u8(4)?;
                super::write_id(writer, gate_id.as_bytes())?;
                super::write_id(writer, execution_id.as_bytes())?;
                super::write_result(writer, result)?;
            }
            GateCommandKind::ClassifyRecovery { gate_id, execution_id, disposition } => {
                writer.write_u8(5)?;
                super::write_id(writer, gate_id.as_bytes())?;
                super::write_id(writer, execution_id.as_bytes())?;
                writer.write_u8(match disposition {
                    crate::RecoveryDisposition::SafeToRetry => 1,
                    crate::RecoveryDisposition::TerminalFailure => 2,
                    crate::RecoveryDisposition::StillActive => 3,
                })?;
            }
            GateCommandKind::PublishEvidence { gate_id, execution_id, receipt } => {
                writer.write_u8(6)?;
                super::write_id(writer, gate_id.as_bytes())?;
                super::write_id(writer, execution_id.as_bytes())?;
                super::write_receipt(writer, receipt)?;
            }
            GateCommandKind::BeginCancellation => writer.write_u8(7)?,
            GateCommandKind::FinalizeRun => writer.write_u8(8)?,
            GateCommandKind::PauseRun => writer.write_u8(9)?,
            GateCommandKind::ResumeRun => writer.write_u8(10)?,
        }
        Ok(())
    }
}

impl CanonicalDecode for GateCommandFrame {
    const FAMILY: u16 = 50;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::read_command_id(reader)?;
        let event_id = super::read_event_id(reader)?;
        let run_id = super::read_run_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let previous = super::read_option_event(reader)?;
        if (expected_sequence == 0) != previous.is_none() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()));
        }
        let prior = super::read_digest(reader)?;
        let revision = super::read_revision(reader)?;
        let kind = read_kind(reader)?;
        GateCommand::new(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            previous,
            prior,
            revision,
            kind,
        )
        .map(Self)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()))
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<GateCommandKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(GateCommandKind::StartRun { snapshot_digest: super::read_digest(reader)? }),
        2 => Ok(GateCommandKind::PrepareAttempt {
            gate_id: super::read_gate_id(reader)?,
            attempt: super::read_attempt(reader)?,
        }),
        3 => Ok(GateCommandKind::MarkDispatched {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
        }),
        4 => Ok(GateCommandKind::ObserveResult {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
            result: super::read_result(reader)?,
        }),
        5 => Ok(GateCommandKind::ClassifyRecovery {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
            disposition: read_recovery(reader)?,
        }),
        6 => Ok(GateCommandKind::PublishEvidence {
            gate_id: super::read_gate_id(reader)?,
            execution_id: super::read_execution_id(reader)?,
            receipt: super::read_receipt(reader)?,
        }),
        7 => Ok(GateCommandKind::BeginCancellation),
        8 => Ok(GateCommandKind::FinalizeRun),
        9 => Ok(GateCommandKind::PauseRun),
        10 => Ok(GateCommandKind::ResumeRun),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn read_recovery(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::RecoveryDisposition, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(crate::RecoveryDisposition::SafeToRetry),
        2 => Ok(crate::RecoveryDisposition::TerminalFailure),
        3 => Ok(crate::RecoveryDisposition::StillActive),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
