//! Canonical reducer invocation envelope.

#![allow(
    clippy::missing_errors_doc,
    reason = "canonical envelope failures use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use crate::primitive::{
    read_id, read_option_id, read_revision, write_id, write_option_id, write_revision,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};
use peritus_kernel::CommandEnvelope;
use peritus_types::{CommandId, EventId};

/// Exact idempotency and causal-head request metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommandEnvelopeDto(CommandEnvelope);

impl CommandEnvelopeDto {
    /// Borrows the checked reducer envelope.
    #[must_use]
    pub const fn as_domain(&self) -> &CommandEnvelope {
        &self.0
    }

    /// Consumes the DTO as a checked reducer envelope.
    #[must_use]
    pub const fn into_domain(self) -> CommandEnvelope {
        self.0
    }
}

impl From<CommandEnvelope> for CommandEnvelopeDto {
    fn from(envelope: CommandEnvelope) -> Self {
        Self(envelope)
    }
}

impl CanonicalEncode for CommandEnvelopeDto {
    const FAMILY: u16 = 2;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.0.command_id().as_bytes())?;
        write_id(writer, self.0.event_id().as_bytes())?;
        write_option_id(writer, self.0.expected_previous_event_id(), EventId::into_bytes)?;
        write_revision(writer, &self.0.revision())
    }
}

impl CanonicalDecode for CommandEnvelopeDto {
    const FAMILY: u16 = 2;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        Ok(Self(CommandEnvelope::new(
            read_id(reader, CommandId::new)?,
            read_id(reader, EventId::new)?,
            read_option_id(reader, EventId::new)?,
            read_revision(reader)?,
        )))
    }
}
