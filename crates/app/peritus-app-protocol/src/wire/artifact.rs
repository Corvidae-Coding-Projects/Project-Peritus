//! Canonical artifact-transfer value helpers.

use crate::{
    AppProtocolLimits, ArtifactCancellation, ArtifactChunk, ArtifactCompletion, ArtifactMetadata,
    CanonicalMediaType, TransferId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::ArtifactId;

use super::primitive::{invalid, read_digest, read_id, write_digest, write_id};

pub(super) fn write_artifact_metadata(
    writer: &mut CanonicalWriter,
    value: &ArtifactMetadata,
) -> Result<(), CodecError> {
    write_id(writer, value.transfer_id().as_bytes())?;
    write_id(writer, value.artifact_id().as_bytes())?;
    writer.write_u64(value.byte_size())?;
    writer.write_str(value.media_type().as_str())?;
    write_digest(writer, value.digest())?;
    writer.write_u32(value.preferred_chunk_size())
}

pub(super) fn read_artifact_metadata(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ArtifactMetadata, CodecError> {
    let offset = reader.offset();
    let transfer_id = read_id(reader, TransferId::new)?;
    let artifact_id = read_id(reader, ArtifactId::new)?;
    let byte_size = reader.read_u64()?;
    let media_offset = reader.offset();
    let media_type = invalid(
        media_offset,
        CanonicalMediaType::new(reader.read_str()?.to_owned(), limits.codec().max_string_bytes),
    )?;
    let digest = read_digest(reader)?;
    let preferred_chunk_size = reader.read_u32()?;
    let preferred_fits = usize::try_from(preferred_chunk_size)
        .is_ok_and(|size| size <= limits.max_artifact_chunk_bytes());
    if !preferred_fits {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(
        offset,
        ArtifactMetadata::new(
            transfer_id,
            artifact_id,
            byte_size,
            media_type,
            digest,
            preferred_chunk_size,
            limits.max_artifact_chunk_bytes(),
        ),
    )
}

pub(super) fn write_artifact_chunk(
    writer: &mut CanonicalWriter,
    value: &ArtifactChunk,
) -> Result<(), CodecError> {
    write_id(writer, value.transfer_id().as_bytes())?;
    write_id(writer, value.artifact_id().as_bytes())?;
    writer.write_u64(value.ordinal())?;
    writer.write_u64(value.offset())?;
    writer.write_bytes(value.bytes())
}

pub(super) fn read_artifact_chunk(
    reader: &mut CanonicalReader<'_>,
    limits: AppProtocolLimits,
) -> Result<ArtifactChunk, CodecError> {
    let offset = reader.offset();
    let transfer_id = read_id(reader, TransferId::new)?;
    let artifact_id = read_id(reader, ArtifactId::new)?;
    let ordinal = reader.read_u64()?;
    let byte_offset = reader.read_u64()?;
    let bytes = reader.read_bytes_owned()?;
    if bytes.len() > limits.max_artifact_chunk_bytes() {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(
        offset,
        ArtifactChunk::new(
            transfer_id,
            artifact_id,
            ordinal,
            byte_offset,
            bytes,
            limits.max_artifact_chunk_bytes(),
        ),
    )
}

pub(super) fn write_artifact_cancellation(
    writer: &mut CanonicalWriter,
    value: ArtifactCancellation,
) -> Result<(), CodecError> {
    write_id(writer, value.transfer_id().as_bytes())?;
    write_id(writer, value.artifact_id().as_bytes())?;
    write_id(writer, value.correlation_id().as_bytes())
}

pub(super) fn read_artifact_cancellation(
    reader: &mut CanonicalReader<'_>,
) -> Result<ArtifactCancellation, CodecError> {
    Ok(ArtifactCancellation::new(
        read_id(reader, TransferId::new)?,
        read_id(reader, ArtifactId::new)?,
        read_id(reader, crate::CorrelationId::new)?,
    ))
}

pub(super) fn write_artifact_completion(
    writer: &mut CanonicalWriter,
    value: ArtifactCompletion,
) -> Result<(), CodecError> {
    write_id(writer, value.transfer_id().as_bytes())?;
    write_id(writer, value.artifact_id().as_bytes())?;
    writer.write_u64(value.byte_size())?;
    write_digest(writer, value.digest())
}

pub(super) fn read_artifact_completion(
    reader: &mut CanonicalReader<'_>,
) -> Result<ArtifactCompletion, CodecError> {
    Ok(ArtifactCompletion::new(
        read_id(reader, TransferId::new)?,
        read_id(reader, ArtifactId::new)?,
        reader.read_u64()?,
        read_digest(reader)?,
    ))
}
