//! Shared exact primitive encoding for E1-owned values.

use peritus_codec::{CanonicalReader, CodecError, CodecErrorKind};
use peritus_types::{CommandId, EventId, Sha256Digest};

pub(super) fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

pub(super) fn read_command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    let offset = reader.offset();
    CommandId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn read_event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    let offset = reader.offset();
    EventId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

pub(super) const fn unknown(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, offset)
}
