//! Exact primitive readers shared by E3 wire families.

use peritus_codec::{CanonicalReader, CodecError, CodecErrorKind};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::EvaluationCampaignId;

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
pub(super) fn campaign_id(
    reader: &mut CanonicalReader<'_>,
) -> Result<EvaluationCampaignId, CodecError> {
    let offset = reader.offset();
    EvaluationCampaignId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
pub(super) const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}
pub(super) fn semantic(_: impl core::fmt::Display) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, 0)
}
