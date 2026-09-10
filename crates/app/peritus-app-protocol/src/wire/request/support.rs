//! Subscription and artifact-open request codecs.

use crate::{
    AppProtocolLimits, ArtifactOpenRequest, EventCursor, SubscriptionId, SubscriptionRequest,
    TransferId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::ArtifactId;

use super::super::{
    primitive::{invalid, read_id, write_id},
    subscription::{read_filter, write_filter},
};

pub(super) fn write_subscription_request(
    writer: &mut CanonicalWriter,
    value: &SubscriptionRequest,
) -> Result<(), CodecError> {
    write_id(writer, value.subscription_id().as_bytes())?;
    write_filter(writer, value.filter())?;
    writer.write_u64(value.after().get())?;
    writer.write_u32(value.maximum_in_flight())?;
    writer.write_bool(value.snapshot_acceptable())
}

pub(super) fn read_subscription_request(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<SubscriptionRequest, CodecError> {
    let offset = reader.offset();
    let subscription_id = read_id(reader, SubscriptionId::new)?;
    let filter = read_filter(reader, limits)?;
    let after = EventCursor::new(reader.read_u64()?);
    let maximum = reader.read_u32()?;
    let maximum_usize = usize::try_from(maximum)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
    if maximum_usize > limits.max_in_flight_events() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let snapshot_acceptable = reader.read_bool()?;
    invalid(
        offset,
        SubscriptionRequest::new(subscription_id, filter, after, maximum, snapshot_acceptable),
    )
}

pub(super) fn write_artifact_open(
    writer: &mut CanonicalWriter,
    value: ArtifactOpenRequest,
) -> Result<(), CodecError> {
    write_id(writer, value.transfer_id().as_bytes())?;
    write_id(writer, value.artifact_id().as_bytes())
}

pub(super) fn read_artifact_open(
    reader: &mut CanonicalReader<'_>,
) -> Result<ArtifactOpenRequest, CodecError> {
    Ok(ArtifactOpenRequest::new(
        read_id(reader, TransferId::new)?,
        read_id(reader, ArtifactId::new)?,
    ))
}
