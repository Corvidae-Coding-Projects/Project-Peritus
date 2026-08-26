//! Canonical scalar and aggregate helpers shared by A3 wire families.

use crate::{
    AppProtocolLimits, ProtocolContext, ProtocolFeatureName, ProtocolFeatureSet, ProtocolId,
    ProtocolVersion, VersionRange,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, SessionId, Sha256Digest, WorkspaceId,
};

pub(super) fn invalid<T, E>(offset: usize, result: Result<T, E>) -> Result<T, CodecError> {
    result.map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) const fn unknown<T>(offset: usize) -> Result<T, CodecError> {
    Err(CodecError::at(CodecErrorKind::UnknownTag, offset))
}

pub(super) fn write_id(writer: &mut CanonicalWriter, bytes: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(bytes)
}

pub(super) fn read_id<T>(
    reader: &mut CanonicalReader<'_>,
    checked: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    invalid(offset, checked(reader.read_fixed()?))
}

pub(super) fn write_digest(
    writer: &mut CanonicalWriter,
    value: Sha256Digest,
) -> Result<(), CodecError> {
    writer.write_fixed(value.as_bytes())
}

pub(super) fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

pub(super) fn write_usize(writer: &mut CanonicalWriter, value: usize) -> Result<(), CodecError> {
    let value = u64::try_from(value)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, writer.len()))?;
    writer.write_u64(value)
}

pub(super) fn read_usize(reader: &mut CanonicalReader<'_>) -> Result<usize, CodecError> {
    let offset = reader.offset();
    usize::try_from(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))
}

pub(super) fn write_revision(
    writer: &mut CanonicalWriter,
    value: RevisionTuple,
) -> Result<(), CodecError> {
    write_id(writer, value.acceptance_spec_id().as_bytes())?;
    write_id(writer, value.harness_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    writer.write_u64(value.workspace_generation().get())?;
    writer.write_u64(value.workspace_revision().get())?;
    write_id(writer, value.policy_id().as_bytes())?;
    write_id(writer, value.provider_profile_id().as_bytes())
}

pub(super) fn read_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, CodecError> {
    let acceptance_spec_id = read_id(reader, AcceptanceSpecId::new)?;
    let harness_id = read_id(reader, HarnessId::new)?;
    let workspace_id = read_id(reader, WorkspaceId::new)?;
    let generation_offset = reader.offset();
    let generation = invalid(generation_offset, Generation::new(reader.read_u64()?))?;
    let revision_offset = reader.offset();
    let revision = invalid(revision_offset, RevisionNumber::new(reader.read_u64()?))?;
    Ok(RevisionTuple::new(
        acceptance_spec_id,
        harness_id,
        workspace_id,
        generation,
        revision,
        read_id(reader, PolicyId::new)?,
        read_id(reader, ProviderProfileId::new)?,
    ))
}

pub(super) fn write_option_revision(
    writer: &mut CanonicalWriter,
    value: Option<RevisionTuple>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_revision(writer, value)?;
    }
    Ok(())
}

pub(super) fn read_option_revision(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<RevisionTuple>, CodecError> {
    if reader.read_option_tag()? { read_revision(reader).map(Some) } else { Ok(None) }
}

pub(super) fn write_version(
    writer: &mut CanonicalWriter,
    value: ProtocolVersion,
) -> Result<(), CodecError> {
    writer.write_u16(value.major())?;
    writer.write_u16(value.minor())
}

pub(super) fn read_version(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProtocolVersion, CodecError> {
    let offset = reader.offset();
    let major = reader.read_u16()?;
    let minor = reader.read_u16()?;
    invalid(offset, ProtocolVersion::new(major, minor))
}

pub(super) fn write_ranges(
    writer: &mut CanonicalWriter,
    values: &[VersionRange],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        writer.write_u16(value.major())?;
        writer.write_u16(value.minor_min())?;
        writer.write_u16(value.minor_max())?;
    }
    Ok(())
}

pub(super) fn read_ranges(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
) -> Result<Vec<VersionRange>, CodecError> {
    let offset = reader.offset();
    let length = reader.read_collection_len()?;
    if length > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    if length == 0 {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        let item_offset = reader.offset();
        let value = invalid(
            item_offset,
            VersionRange::new(reader.read_u16()?, reader.read_u16()?, reader.read_u16()?),
        )?;
        let invalid_order = values.last().is_some_and(|previous: &VersionRange| {
            if previous >= &value {
                true
            } else if previous.major() == value.major() {
                value.minor_min() <= previous.minor_max()
            } else {
                false
            }
        });
        if invalid_order {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, item_offset));
        }
        values.push(value);
    }
    Ok(values)
}

pub(super) fn write_features(
    writer: &mut CanonicalWriter,
    values: &ProtocolFeatureSet,
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values.as_slice() {
        writer.write_str(value.as_str())?;
    }
    Ok(())
}

pub(super) fn read_features(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
) -> Result<ProtocolFeatureSet, CodecError> {
    let collection_offset = reader.offset();
    let length = reader.read_collection_len()?;
    if length > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, collection_offset));
    }
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        let item_offset = reader.offset();
        let value = invalid(item_offset, ProtocolFeatureName::new(reader.read_str()?.to_owned()))?;
        if values.last().is_some_and(|previous: &ProtocolFeatureName| {
            previous.canonical_cmp(&value) != core::cmp::Ordering::Less
        }) {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, item_offset));
        }
        values.push(value);
    }
    invalid(collection_offset, ProtocolFeatureSet::new(values, maximum))
}

pub(super) fn write_context(
    writer: &mut CanonicalWriter,
    value: ProtocolContext,
) -> Result<(), CodecError> {
    write_id(writer, value.protocol_id().as_bytes())?;
    write_version(writer, value.version())?;
    write_id(writer, value.session_id().as_bytes())
}

pub(super) fn read_context(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProtocolContext, CodecError> {
    Ok(ProtocolContext::new(
        read_id(reader, ProtocolId::new)?,
        read_version(reader)?,
        read_id(reader, SessionId::new)?,
    ))
}

pub(super) fn write_limits(
    writer: &mut CanonicalWriter,
    value: AppProtocolLimits,
) -> Result<(), CodecError> {
    let codec = value.codec();
    for item in [
        codec.max_frame_bytes,
        codec.max_payload_bytes,
        codec.max_collection_items,
        codec.max_string_bytes,
        codec.max_opaque_bytes,
    ] {
        write_usize(writer, item)?;
    }
    writer.write_u16(codec.max_nesting_depth)?;
    for item in [
        value.max_versions(),
        value.max_features(),
        value.max_idempotency_entries(),
        value.max_topics(),
        value.max_in_flight_events(),
        value.max_artifact_chunk_bytes(),
        value.max_prompt_choices(),
        value.max_terminal_chunk_bytes(),
        value.max_diagnostic_bytes(),
        value.max_remaining_work_items(),
    ] {
        write_usize(writer, item)?;
    }
    Ok(())
}

pub(super) fn read_limits(
    reader: &mut CanonicalReader<'_>,
) -> Result<AppProtocolLimits, CodecError> {
    let offset = reader.offset();
    let codec = CodecLimits::new(
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        reader.read_u16()?,
    );
    let result = AppProtocolLimits::new(
        codec,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
        read_usize(reader)?,
    );
    invalid(offset, result)
}

pub(super) fn write_string_option(
    writer: &mut CanonicalWriter,
    value: Option<&str>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_str(value)?;
    }
    Ok(())
}

pub(super) fn read_string_option(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<String>, CodecError> {
    if reader.read_option_tag()? { Ok(Some(reader.read_str()?.to_owned())) } else { Ok(None) }
}
