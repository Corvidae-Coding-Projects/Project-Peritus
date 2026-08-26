//! Exact primitive readers shared by E2 wire families.

use peritus_codec::{CanonicalReader, CodecError, CodecErrorKind};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::DebuggerJobId;

pub(super) fn digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

pub(super) fn command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    let offset = reader.offset();
    CommandId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    let offset = reader.offset();
    EventId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn job_id(reader: &mut CanonicalReader<'_>) -> Result<DebuggerJobId, CodecError> {
    let offset = reader.offset();
    DebuggerJobId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

pub(super) fn semantic(_error: impl core::fmt::Display) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, 0)
}
