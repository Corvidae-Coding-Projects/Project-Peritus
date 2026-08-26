//! Inert canonical family-82 debugger command frames.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{
    DebuggerCommand, DebuggerError, DebuggerErrorKind, DebuggerJobId, DebuggerOperation,
    DebuggerRecovery,
};

/// Canonical inert family-82 schema-v1 command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerCommandFrame {
    command_id: CommandId,
    event_id: EventId,
    job_id: DebuggerJobId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    query_digest: Sha256Digest,
    command_digest: Sha256Digest,
    kind_bytes: Vec<u8>,
}

impl DebuggerCommandFrame {
    /// Converts one checked command into an inert frame.
    ///
    /// # Errors
    ///
    /// Returns a codec error when the semantic command exceeds family-82 bounds.
    pub fn from_command(command: &DebuggerCommand) -> Result<Self, CodecError> {
        Ok(Self {
            command_id: command.command_id(),
            event_id: command.event_id(),
            job_id: command.job_id(),
            expected_sequence: command.expected_sequence(),
            expected_previous_event: command.expected_previous_event(),
            prior_state_digest: command.prior_state_digest(),
            query_digest: command.query_digest(),
            command_digest: command.digest(),
            kind_bytes: super::semantic::encode(command.kind()).map_err(super::scalar::semantic)?,
        })
    }

    /// Activates inert data only through complete semantic constructors.
    ///
    /// # Errors
    ///
    /// Rejects invalid semantic payloads or a command digest that does not match them.
    pub fn check(self) -> Result<DebuggerCommand, DebuggerError> {
        let kind = super::semantic::decode(&self.kind_bytes)?;
        let command = DebuggerCommand::new(
            self.command_id,
            self.event_id,
            self.job_id,
            self.expected_sequence,
            self.expected_previous_event,
            self.prior_state_digest,
            self.query_digest,
            kind,
        )?;
        if command.digest() != self.command_digest {
            return Err(DebuggerError::new(
                DebuggerErrorKind::IdempotencyConflict,
                DebuggerOperation::DecodeProtocol,
                DebuggerRecovery::Quarantine,
                "family-82 command digest disagrees with semantic data",
            ));
        }
        Ok(command)
    }

    /// Command identity without activating semantic data.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
}

impl CanonicalEncode for DebuggerCommandFrame {
    const FAMILY: u16 = 82;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.job_id.as_bytes())?;
        writer.write_u64(self.expected_sequence)?;
        writer.write_option_tag(self.expected_previous_event.is_some())?;
        if let Some(event) = self.expected_previous_event {
            writer.write_fixed(event.as_bytes())?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.query_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for DebuggerCommandFrame {
    const FAMILY: u16 = 82;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::scalar::command_id(reader)?;
        let event_id = super::scalar::event_id(reader)?;
        let job_id = super::scalar::job_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let expected_previous_event =
            reader.read_option_tag()?.then(|| super::scalar::event_id(reader)).transpose()?;
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = super::scalar::digest(reader)?;
        let query_digest = super::scalar::digest(reader)?;
        let command_digest = super::scalar::digest(reader)?;
        let kind_offset = reader.offset();
        let kind_bytes = reader.read_bytes_owned()?;
        if kind_bytes.is_empty() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, kind_offset));
        }
        let _ = super::semantic::decode(&kind_bytes).map_err(super::scalar::semantic)?;
        Ok(Self {
            command_id,
            event_id,
            job_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            query_digest,
            command_digest,
            kind_bytes,
        })
    }
}
