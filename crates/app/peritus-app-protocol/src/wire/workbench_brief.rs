//! Bounded exact brief wire contract; no model suggestions can become confirmed fields.

use super::primitive::{invalid, unknown};
use crate::{
    MAX_WORKBENCH_BRIEF_FIELDS, MAX_WORKBENCH_BRIEF_OBSERVATIONS, MAX_WORKBENCH_BRIEF_PROPOSALS,
    WorkbenchBrief, WorkbenchBriefEntry, WorkbenchBriefField, WorkbenchBriefObservation,
    WorkbenchBriefObservationKind, WorkbenchBriefProposal,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

#[cfg(test)]
mod tests;

pub(super) fn write_field(
    w: &mut CanonicalWriter,
    field: WorkbenchBriefField,
) -> Result<(), CodecError> {
    w.write_u16(match field {
        WorkbenchBriefField::Objective => 1,
        WorkbenchBriefField::Acceptance => 2,
        WorkbenchBriefField::Constraints => 3,
        WorkbenchBriefField::Assumptions => 4,
    })
}
pub(super) fn read_field(r: &mut CanonicalReader<'_>) -> Result<WorkbenchBriefField, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => Ok(WorkbenchBriefField::Objective),
        2 => Ok(WorkbenchBriefField::Acceptance),
        3 => Ok(WorkbenchBriefField::Constraints),
        4 => Ok(WorkbenchBriefField::Assumptions),
        _ => unknown(offset),
    }
}
pub(super) fn write_brief(
    w: &mut CanonicalWriter,
    brief: &WorkbenchBrief,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, brief.query())?;
    w.write_u64(brief.revision())?;
    w.write_u16(
        u16::try_from(brief.entries().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for entry in brief.entries() {
        write_field(w, entry.field())?;
        super::workbench_inputs::write_row(w, entry.source())?;
    }
    w.write_u16(
        u16::try_from(brief.proposals().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for proposal in brief.proposals() {
        super::primitive::write_id(w, proposal.operation().as_bytes())?;
        super::primitive::write_id(w, proposal.invocation().as_bytes())?;
        w.write_fixed(proposal.digest().as_bytes())?;
        w.write_str(proposal.text().as_str())?;
    }
    w.write_u16(
        u16::try_from(brief.observations().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for observation in brief.observations() {
        w.write_u16(match observation.kind() {
            WorkbenchBriefObservationKind::Image => 1,
            WorkbenchBriefObservationKind::File => 2,
        })?;
        super::primitive::write_id(w, observation.operation().as_bytes())?;
        w.write_bool(observation.version().is_some())?;
        if let Some(version) = observation.version() {
            super::primitive::write_id(w, version.as_bytes())?;
        }
        w.write_str(observation.label())?;
        w.write_fixed(observation.digest().as_bytes())?;
        w.write_u64(observation.bytes())?;
        w.write_bool(observation.selected())?;
    }
    w.write_u32(brief.excluded_proposals())?;
    Ok(())
}
pub(super) fn read_brief(r: &mut CanonicalReader<'_>) -> Result<WorkbenchBrief, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_BRIEF_FIELDS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let field = read_field(r)?;
        let row = super::workbench_inputs::read_row(r)?;
        entries.push(invalid(offset, WorkbenchBriefEntry::new(field, row))?);
    }
    let proposal_count = usize::from(r.read_u16()?);
    if proposal_count > MAX_WORKBENCH_BRIEF_PROPOSALS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut proposals = Vec::with_capacity(proposal_count);
    for _ in 0..proposal_count {
        let operation = super::primitive::read_id(r, crate::ControlOperationId::new)?;
        let invocation = super::primitive::read_id(r, crate::WorkbenchInvocationId::new)?;
        let digest = peritus_types::Sha256Digest::new(r.read_fixed()?);
        let text = super::workbench_inputs::read_text(r)?;
        proposals.push(invalid(
            offset,
            WorkbenchBriefProposal::new(operation, invocation, digest, text),
        )?);
    }
    let observation_count = usize::from(r.read_u16()?);
    if observation_count > MAX_WORKBENCH_BRIEF_OBSERVATIONS {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut observations = Vec::with_capacity(observation_count);
    for _ in 0..observation_count {
        let kind = match r.read_u16()? {
            1 => WorkbenchBriefObservationKind::Image,
            2 => WorkbenchBriefObservationKind::File,
            _ => return unknown(offset),
        };
        let operation = super::primitive::read_id(r, crate::ControlOperationId::new)?;
        let version = if r.read_bool()? {
            Some(super::primitive::read_id(r, crate::ControlOperationId::new)?)
        } else {
            None
        };
        let label = r.read_str()?.to_owned();
        let digest = peritus_types::Sha256Digest::new(r.read_fixed()?);
        let bytes = r.read_u64()?;
        let selected = r.read_bool()?;
        observations.push(invalid(
            offset,
            WorkbenchBriefObservation::new(
                kind, operation, version, label, digest, bytes, selected,
            ),
        )?);
    }
    let excluded = r.read_u32()?;
    invalid(
        offset,
        WorkbenchBrief::with_sources(query, revision, entries, proposals, observations, excluded),
    )
}
