//! Additive brief paging codecs; existing complete-brief bytes retain their schema.
use super::primitive::{invalid, read_id, write_id};
use crate::{
    ControlOperationId, MAX_WORKBENCH_BRIEF_FIELDS, WORKBENCH_BRIEF_PAGE_ITEMS,
    WorkbenchBriefEntry, WorkbenchBriefPage, WorkbenchBriefPageRequest, WorkbenchBriefProposalPage,
    WorkbenchBriefProposalReference, WorkbenchBriefProposalRequest, WorkbenchInvocationId,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::Sha256Digest;

pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: WorkbenchBriefPageRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    w.write_u64(value.proposals())?;
    w.write_u64(value.observations())
}
pub(super) fn read_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchBriefPageRequest, CodecError> {
    let offset = r.offset();
    invalid(
        offset,
        WorkbenchBriefPageRequest::new(
            super::workbench::read_query(r)?,
            r.read_u64()?,
            r.read_u64()?,
            r.read_u64()?,
        ),
    )
}
fn write_reference(
    w: &mut CanonicalWriter,
    value: WorkbenchBriefProposalReference,
) -> Result<(), CodecError> {
    write_id(w, value.operation().as_bytes())?;
    write_id(w, value.invocation().as_bytes())?;
    w.write_fixed(value.digest().as_bytes())?;
    w.write_u64(value.bytes())
}
fn read_reference(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchBriefProposalReference, CodecError> {
    let offset = r.offset();
    invalid(
        offset,
        WorkbenchBriefProposalReference::new(
            read_id(r, ControlOperationId::new)?,
            read_id(r, WorkbenchInvocationId::new)?,
            Sha256Digest::new(r.read_fixed()?),
            r.read_u64()?,
        ),
    )
}
pub(super) fn write_proposal_request(
    w: &mut CanonicalWriter,
    value: WorkbenchBriefProposalRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    write_reference(w, value.proposal())?;
    w.write_u64(value.offset())
}
pub(super) fn read_proposal_request(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchBriefProposalRequest, CodecError> {
    let offset = r.offset();
    invalid(
        offset,
        WorkbenchBriefProposalRequest::new(
            super::workbench::read_query(r)?,
            r.read_u64()?,
            read_reference(r)?,
            r.read_u64()?,
        ),
    )
}
pub(super) fn write_proposal_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchBriefProposalPage,
) -> Result<(), CodecError> {
    write_proposal_request(w, value.request())?;
    w.write_str(value.text())
}
pub(super) fn read_proposal_page(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchBriefProposalPage, CodecError> {
    let offset = r.offset();
    invalid(
        offset,
        WorkbenchBriefProposalPage::new(read_proposal_request(r)?, r.read_str()?.to_owned()),
    )
}
pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchBriefPage,
) -> Result<(), CodecError> {
    write_request(w, value.request())?;
    w.write_u64(value.revision())?;
    w.write_u16(u16::try_from(value.entries().len()).expect("validated closed brief fields"))?;
    for entry in value.entries() {
        super::workbench_brief::write_field(w, entry.field())?;
        super::workbench_inputs::write_row(w, entry.source())?;
    }
    w.write_u64(value.proposal_total())?;
    w.write_u16(u16::try_from(value.proposals().len()).expect("validated transport page"))?;
    for proposal in value.proposals() {
        write_reference(w, *proposal)?;
    }
    w.write_u64(value.observation_total())?;
    w.write_u16(u16::try_from(value.observations().len()).expect("validated transport page"))?;
    for observation in value.observations() {
        super::workbench_brief::write_observation(w, observation)?;
    }
    Ok(())
}
pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchBriefPage, CodecError> {
    let offset = r.offset();
    let request = read_request(r)?;
    let revision = r.read_u64()?;
    let count = read_count(r, MAX_WORKBENCH_BRIEF_FIELDS)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(invalid(
            offset,
            WorkbenchBriefEntry::new(
                super::workbench_brief::read_field(r)?,
                super::workbench_inputs::read_row(r)?,
            ),
        )?);
    }
    let proposal_total = r.read_u64()?;
    let count = read_count(r, WORKBENCH_BRIEF_PAGE_ITEMS)?;
    let mut proposals = Vec::with_capacity(count);
    for _ in 0..count {
        proposals.push(read_reference(r)?);
    }
    let observation_total = r.read_u64()?;
    let count = read_count(r, WORKBENCH_BRIEF_PAGE_ITEMS)?;
    let mut observations = Vec::with_capacity(count);
    for _ in 0..count {
        observations.push(super::workbench_brief::read_observation(r)?);
    }
    invalid(
        offset,
        WorkbenchBriefPage::new(
            request,
            revision,
            entries,
            proposal_total,
            proposals,
            observation_total,
            observations,
        ),
    )
}
fn read_count(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let count = usize::from(r.read_u16()?);
    if count > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, r.offset()));
    }
    Ok(count)
}
