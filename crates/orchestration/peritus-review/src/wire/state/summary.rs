//! Canonical quorum, oscillation, and terminal-summary wire encoding.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};
use peritus_spec::ReviewCategory;

use crate::{
    OscillationKind, OscillationReport, QuorumDimension, QuorumReport, ReviewLimits,
    ReviewTerminal, ReviewTerminalKind,
};

pub(super) fn write_quorum(
    writer: &mut CanonicalWriter,
    value: &QuorumReport,
) -> Result<(), CodecError> {
    writer.write_u16(value.submitted_reviews())?;
    writer.write_collection_len(value.covered_categories().len())?;
    for category in value.covered_categories() {
        super::super::write_digest(writer, category.digest())?;
    }
    for dimension in quorum_dimensions() {
        writer.write_bool(value.passes(dimension))?;
    }
    Ok(())
}

pub(super) fn read_quorum(reader: &mut CanonicalReader<'_>) -> Result<QuorumReport, CodecError> {
    Ok(QuorumReport::from_wire(
        reader.read_u16()?,
        super::super::read_digests(reader, ReviewLimits::MAX_CATEGORIES, ReviewCategory::new)?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
    ))
}

const fn quorum_dimensions() -> [QuorumDimension; 9] {
    [
        QuorumDimension::SubmittedReviewCount,
        QuorumDimension::RequiredCategoryCoverage,
        QuorumDimension::DistinctReviewerIdentities,
        QuorumDimension::ProducerIndependence,
        QuorumDimension::DistinctContexts,
        QuorumDimension::DistinctModelFamilies,
        QuorumDimension::DistinctProviders,
        QuorumDimension::NoSharedAncestry,
        QuorumDimension::FreshContext,
    ]
}

pub(super) fn write_oscillation(
    writer: &mut CanonicalWriter,
    value: &OscillationReport,
) -> Result<(), CodecError> {
    writer.write_collection_len(value.kinds().len())?;
    for kind in value.kinds() {
        writer.write_u8(crate::canonical::oscillation_tag(*kind))?;
    }
    writer.write_u16(value.compared_bindings())?;
    writer.write_u16(value.cycles_used())
}

pub(super) fn read_oscillation(
    reader: &mut CanonicalReader<'_>,
) -> Result<OscillationReport, CodecError> {
    let count = super::super::bounded_len(reader, 5)?;
    let mut kinds = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = reader.offset();
        kinds.push(match reader.read_u8()? {
            1 => OscillationKind::RepeatedFindingSet,
            2 => OscillationKind::SeverityStagnation,
            3 => OscillationKind::SeverityRegression,
            4 => OscillationKind::Disagreement,
            5 => OscillationKind::ReviewCyclesExhausted,
            _ => return Err(super::super::unknown(offset)),
        });
    }
    if kinds.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::super::invalid(reader));
    }
    Ok(OscillationReport::from_wire(kinds, reader.read_u16()?, reader.read_u16()?))
}

pub(super) fn write_terminal(
    writer: &mut CanonicalWriter,
    value: &ReviewTerminal,
) -> Result<(), CodecError> {
    writer.write_u8(crate::canonical::terminal_kind_tag(value.kind()))?;
    writer.write_collection_len(value.unconserved_findings().len())?;
    for finding in value.unconserved_findings() {
        super::super::write_id(writer, finding.as_bytes())?;
    }
    write_quorum(writer, value.quorum())?;
    write_oscillation(writer, value.oscillation())?;
    super::super::write_digest(writer, value.cause_digest())?;
    super::super::write_digest(writer, value.digest())
}

pub(super) fn read_terminal(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReviewTerminal, CodecError> {
    let offset = reader.offset();
    let kind = match reader.read_u8()? {
        1 => ReviewTerminalKind::Completed,
        2 => ReviewTerminalKind::NeedsHuman,
        3 => ReviewTerminalKind::Failed,
        4 => ReviewTerminalKind::Cancelled,
        _ => return Err(super::super::unknown(offset)),
    };
    let count = super::super::bounded_len(reader, ReviewLimits::MAX_FINDINGS as usize)?;
    let mut findings = Vec::with_capacity(count);
    for _ in 0..count {
        findings.push(super::super::read_finding_id(reader)?);
    }
    if findings.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::super::invalid(reader));
    }
    Ok(ReviewTerminal::from_wire(
        kind,
        findings,
        read_quorum(reader)?,
        read_oscillation(reader)?,
        super::super::read_digest(reader)?,
        super::super::read_digest(reader)?,
    ))
}
