//! Canonical structured-finding wire encoding.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_spec::{RequirementId, ReviewCategory};
use peritus_types::FindingId;

use crate::{Confidence, Finding, FindingLocation, FindingSource, ReviewLimits};

pub(super) fn write_finding(
    writer: &mut CanonicalWriter,
    value: &Finding,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.id().as_bytes())?;
    write_source(writer, value.origin())?;
    writer.write_collection_len(value.sources().len())?;
    for source in value.sources() {
        write_source(writer, *source)?;
    }
    super::super::write_digest(writer, value.category().digest())?;
    writer.write_u8(crate::canonical::severity_tag(value.severity()))?;
    writer.write_bool(value.blocking())?;
    writer.write_u16(value.confidence().get())?;
    writer.write_collection_len(value.requirements().len())?;
    for requirement in value.requirements() {
        super::super::write_digest(writer, requirement.digest())?;
    }
    writer.write_collection_len(value.locations().len())?;
    for location in value.locations() {
        write_location(writer, location)?;
    }
    writer.write_collection_len(value.evidence().len())?;
    for evidence in value.evidence() {
        super::super::write_id(writer, evidence.as_bytes())?;
    }
    writer.write_str(value.description())?;
    writer.write_str(value.reproduction())?;
    writer.write_str(value.expected_behavior())?;
    writer.write_str(value.remediation())?;
    super::super::write_revision(writer, value.revision())?;
    super::super::write_digest(writer, value.normalized_digest())?;
    writer.write_collection_len(value.dispositions().len())?;
    for record in value.dispositions() {
        super::disposition::write_disposition(writer, record)?;
    }
    super::super::write_option_id(writer, value.superseded_by(), FindingId::into_bytes)
}

pub(super) fn read_finding(reader: &mut CanonicalReader<'_>) -> Result<Finding, CodecError> {
    let id = super::super::read_finding_id(reader)?;
    let origin = read_source(reader)?;
    let source_count =
        super::super::bounded_len(reader, usize::from(ReviewLimits::MAX_PROVENANCE_SOURCES))?;
    let mut sources = Vec::with_capacity(source_count);
    for _ in 0..source_count {
        sources.push(read_source(reader)?);
    }
    let category = ReviewCategory::new(super::super::read_digest(reader)?);
    let severity = super::super::read_severity(reader)?;
    let blocking = reader.read_bool()?;
    let confidence_offset = reader.offset();
    let confidence_value = reader.read_u16()?;
    if confidence_value > Confidence::MAXIMUM {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, confidence_offset));
    }
    let requirements =
        super::super::read_digests(reader, ReviewLimits::MAX_REQUIREMENTS, RequirementId::new)?;
    let location_count =
        super::super::bounded_len(reader, usize::from(ReviewLimits::MAX_LOCATIONS))?;
    let mut locations = Vec::with_capacity(location_count);
    for _ in 0..location_count {
        locations.push(read_location(reader)?);
    }
    let evidence = super::disposition::read_evidence(reader)?;
    let description = super::super::read_text(reader, ReviewLimits::MAX_TEXT_BYTES)?;
    let reproduction = super::super::read_text(reader, ReviewLimits::MAX_TEXT_BYTES)?;
    let expected = super::super::read_text(reader, ReviewLimits::MAX_TEXT_BYTES)?;
    let remediation = super::super::read_text(reader, ReviewLimits::MAX_TEXT_BYTES)?;
    let revision = super::super::read_revision(reader)?;
    let normalized = super::super::read_digest(reader)?;
    let disposition_count =
        super::super::bounded_len(reader, usize::from(ReviewLimits::MAX_DISPOSITION_RECORDS))?;
    let mut dispositions = Vec::with_capacity(disposition_count);
    for _ in 0..disposition_count {
        dispositions.push(super::disposition::read_disposition(reader)?);
    }
    let superseded =
        reader.read_option_tag()?.then(|| super::super::read_finding_id(reader)).transpose()?;
    Ok(Finding::from_wire(
        id,
        origin,
        sources,
        category,
        severity,
        blocking,
        Confidence::from_wire(confidence_value),
        requirements,
        locations,
        evidence,
        description,
        reproduction,
        expected,
        remediation,
        revision,
        normalized,
        dispositions,
        superseded,
    ))
}

fn write_source(writer: &mut CanonicalWriter, value: FindingSource) -> Result<(), CodecError> {
    super::super::write_id(writer, value.cycle_id().as_bytes())?;
    super::super::write_id(writer, value.reviewer().as_bytes())
}

fn read_source(reader: &mut CanonicalReader<'_>) -> Result<FindingSource, CodecError> {
    Ok(FindingSource::new(
        super::super::read_cycle_id(reader)?,
        super::super::read_actor_id(reader)?,
    ))
}

fn write_location(writer: &mut CanonicalWriter, value: &FindingLocation) -> Result<(), CodecError> {
    writer.write_str(value.path())?;
    writer.write_u32(value.start_line())?;
    writer.write_u32(value.start_column())?;
    writer.write_u32(value.end_line())?;
    writer.write_u32(value.end_column())
}

fn read_location(reader: &mut CanonicalReader<'_>) -> Result<FindingLocation, CodecError> {
    let value = FindingLocation::from_wire(
        super::super::read_text(reader, ReviewLimits::MAX_PATH_BYTES)?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
    );
    value.validate(super::super::production_limits()).map_err(|_| super::super::invalid(reader))?;
    Ok(value)
}
