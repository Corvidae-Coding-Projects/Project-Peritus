//! D2-owned typed B3 codecs for reserved families 53, 54, and 55.

mod command;
mod event;
mod state;

#[cfg(test)]
mod fixture_tests;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_quality_policy::ReviewerIdentity;
use peritus_role::ReviewIndependenceView;
use peritus_spec::{
    ContentReference, EvidenceRequirementId, FindingSeverity, ReviewCategory, ReviewerIndependence,
    WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, ApprovalRequestId, CommandId, EventId, EvidenceId, FindingId,
    Generation, HarnessId, PolicyId, ProviderProfileId, ReviewCycleId, RevisionNumber,
    RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use crate::{ReviewBinding, ReviewLimits};

pub use command::ReviewCommandFrame;
pub use event::ReviewEventFrame;
pub use state::ReviewStateFrame;

const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

const fn unknown(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, offset)
}

fn write_id(writer: &mut CanonicalWriter, value: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(value)
}

fn read_nominal<T>(
    reader: &mut CanonicalReader<'_>,
    construct: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    construct(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

fn read_acceptance_id(reader: &mut CanonicalReader<'_>) -> Result<AcceptanceSpecId, CodecError> {
    read_nominal(reader, AcceptanceSpecId::new)
}

fn read_actor_id(reader: &mut CanonicalReader<'_>) -> Result<ActorId, CodecError> {
    read_nominal(reader, ActorId::new)
}

fn read_approval_id(reader: &mut CanonicalReader<'_>) -> Result<ApprovalRequestId, CodecError> {
    read_nominal(reader, ApprovalRequestId::new)
}

fn read_command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    read_nominal(reader, CommandId::new)
}

fn read_event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    read_nominal(reader, EventId::new)
}

fn read_evidence_id(reader: &mut CanonicalReader<'_>) -> Result<EvidenceId, CodecError> {
    read_nominal(reader, EvidenceId::new)
}

fn read_finding_id(reader: &mut CanonicalReader<'_>) -> Result<FindingId, CodecError> {
    read_nominal(reader, FindingId::new)
}

fn read_harness_id(reader: &mut CanonicalReader<'_>) -> Result<HarnessId, CodecError> {
    read_nominal(reader, HarnessId::new)
}

fn read_policy_id(reader: &mut CanonicalReader<'_>) -> Result<PolicyId, CodecError> {
    read_nominal(reader, PolicyId::new)
}

fn read_provider_profile_id(
    reader: &mut CanonicalReader<'_>,
) -> Result<ProviderProfileId, CodecError> {
    read_nominal(reader, ProviderProfileId::new)
}

fn read_cycle_id(reader: &mut CanonicalReader<'_>) -> Result<ReviewCycleId, CodecError> {
    read_nominal(reader, ReviewCycleId::new)
}

fn read_run_id(reader: &mut CanonicalReader<'_>) -> Result<RunId, CodecError> {
    read_nominal(reader, RunId::new)
}

fn read_workspace_id(reader: &mut CanonicalReader<'_>) -> Result<WorkspaceId, CodecError> {
    read_nominal(reader, WorkspaceId::new)
}

fn write_digest(writer: &mut CanonicalWriter, value: Sha256Digest) -> Result<(), CodecError> {
    writer.write_fixed(value.as_bytes())
}

fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

fn read_text(reader: &mut CanonicalReader<'_>, maximum: u32) -> Result<String, CodecError> {
    let offset = reader.offset();
    let value = reader.read_str()?;
    if value.is_empty() || value.len() > maximum as usize {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(value.to_owned())
}

fn write_revision(writer: &mut CanonicalWriter, revision: RevisionTuple) -> Result<(), CodecError> {
    write_id(writer, revision.acceptance_spec_id().as_bytes())?;
    write_id(writer, revision.harness_id().as_bytes())?;
    write_id(writer, revision.workspace_id().as_bytes())?;
    writer.write_u64(revision.workspace_generation().get())?;
    writer.write_u64(revision.workspace_revision().get())?;
    write_id(writer, revision.policy_id().as_bytes())?;
    write_id(writer, revision.provider_profile_id().as_bytes())
}

fn read_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, CodecError> {
    let acceptance = read_acceptance_id(reader)?;
    let harness = read_harness_id(reader)?;
    let workspace = read_workspace_id(reader)?;
    let generation_offset = reader.offset();
    let generation = Generation::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, generation_offset))?;
    let revision_offset = reader.offset();
    let revision = RevisionNumber::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, revision_offset))?;
    Ok(RevisionTuple::new(
        acceptance,
        harness,
        workspace,
        generation,
        revision,
        read_policy_id(reader)?,
        read_provider_profile_id(reader)?,
    ))
}

fn write_option_id<T>(
    writer: &mut CanonicalWriter,
    value: Option<T>,
    bytes: impl FnOnce(T) -> [u8; 16],
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_id(writer, &bytes(value))?;
    }
    Ok(())
}

fn write_independence(
    writer: &mut CanonicalWriter,
    value: ReviewIndependenceView,
) -> Result<(), CodecError> {
    writer.write_bool(value.distinct_reviewers())?;
    writer.write_bool(value.independent_from_producer())?;
    writer.write_bool(value.distinct_contexts())?;
    writer.write_bool(value.distinct_model_families())?;
    writer.write_bool(value.distinct_providers())?;
    writer.write_bool(value.no_shared_ancestry())?;
    writer.write_bool(value.fresh_context())
}

fn read_independence(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReviewIndependenceView, CodecError> {
    let requirements = ReviewerIndependence::new(
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
        reader.read_bool()?,
    );
    if !reader.read_bool()? {
        return Err(invalid(reader));
    }
    Ok(ReviewIndependenceView::from_contract(requirements))
}

fn read_severity(reader: &mut CanonicalReader<'_>) -> Result<FindingSeverity, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(FindingSeverity::Advisory),
        2 => Ok(FindingSeverity::Low),
        3 => Ok(FindingSeverity::Medium),
        4 => Ok(FindingSeverity::High),
        5 => Ok(FindingSeverity::Critical),
        _ => Err(unknown(offset)),
    }
}

fn write_limits(writer: &mut CanonicalWriter, value: ReviewLimits) -> Result<(), CodecError> {
    writer.write_u16(value.cycles())?;
    writer.write_u16(value.assignments())?;
    writer.write_u16(value.submissions())?;
    writer.write_u32(value.findings())?;
    writer.write_u16(value.categories())?;
    writer.write_u16(value.requirements())?;
    writer.write_u16(value.locations())?;
    writer.write_u16(value.evidence_references())?;
    writer.write_u16(value.provenance_sources())?;
    writer.write_u16(value.disposition_records())?;
    writer.write_u32(value.path_bytes())?;
    writer.write_u32(value.text_bytes())?;
    writer.write_u32(value.opaque_bytes())?;
    writer.write_u64(value.payload_bytes())?;
    writer.write_u64(value.state_bytes())
}

fn read_limits(reader: &mut CanonicalReader<'_>) -> Result<ReviewLimits, CodecError> {
    let offset = reader.offset();
    ReviewLimits::new(
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u32()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u64()?,
        reader.read_u64()?,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

const fn production_limits() -> ReviewLimits {
    ReviewLimits::from_wire(
        ReviewLimits::MAX_CYCLES,
        ReviewLimits::MAX_ASSIGNMENTS,
        ReviewLimits::MAX_SUBMISSIONS,
        ReviewLimits::MAX_FINDINGS,
        ReviewLimits::MAX_CATEGORIES,
        ReviewLimits::MAX_REQUIREMENTS,
        ReviewLimits::MAX_LOCATIONS,
        ReviewLimits::MAX_EVIDENCE_REFERENCES,
        ReviewLimits::MAX_PROVENANCE_SOURCES,
        ReviewLimits::MAX_DISPOSITION_RECORDS,
        ReviewLimits::MAX_PATH_BYTES,
        ReviewLimits::MAX_TEXT_BYTES,
        ReviewLimits::MAX_OPAQUE_BYTES,
        ReviewLimits::MAX_PAYLOAD_BYTES,
        ReviewLimits::MAX_STATE_BYTES,
    )
}

fn write_binding(writer: &mut CanonicalWriter, value: &ReviewBinding) -> Result<(), CodecError> {
    write_id(writer, value.contract_id().as_bytes())?;
    write_digest(writer, value.contract_digest())?;
    write_revision(writer, value.revision())?;
    writer.write_collection_len(value.required_categories().len())?;
    for category in value.required_categories() {
        write_digest(writer, category.digest())?;
    }
    writer.write_u16(value.reviewer_quorum())?;
    write_independence(writer, value.independence())?;
    writer.write_u8(crate::canonical::severity_tag(value.blocking_severity()))?;
    writer.write_u16(value.maximum_cycles())?;
    match value.waiver_policy() {
        WaiverPolicy::Forbidden => writer.write_u8(1)?,
        WaiverPolicy::Allowed { authority, evidence } => {
            writer.write_u8(2)?;
            write_digest(writer, authority.digest())?;
            write_digest(writer, evidence.digest())?;
        }
    }
    write_digest(writer, value.candidate_digest())?;
    write_digest(writer, value.tree_digest())?;
    writer.write_collection_len(value.producer_actors().len())?;
    for actor in value.producer_actors() {
        write_id(writer, actor.as_bytes())?;
    }
    writer.write_collection_len(value.producer_ancestries().len())?;
    for ancestry in value.producer_ancestries() {
        write_digest(writer, *ancestry)?;
    }
    write_digest(writer, value.digest())
}

fn read_binding(reader: &mut CanonicalReader<'_>) -> Result<ReviewBinding, CodecError> {
    let contract_id = read_acceptance_id(reader)?;
    let contract_digest = read_digest(reader)?;
    let revision = read_revision(reader)?;
    let categories = read_digests(reader, ReviewLimits::MAX_CATEGORIES, ReviewCategory::new)?;
    let reviewer_quorum = reader.read_u16()?;
    let independence = read_independence(reader)?;
    let severity = read_severity(reader)?;
    let maximum_cycles = reader.read_u16()?;
    let waiver_offset = reader.offset();
    let waiver = match reader.read_u8()? {
        1 => WaiverPolicy::Forbidden,
        2 => WaiverPolicy::Allowed {
            authority: ContentReference::new(read_digest(reader)?),
            evidence: EvidenceRequirementId::new(read_digest(reader)?),
        },
        _ => return Err(unknown(waiver_offset)),
    };
    let candidate = read_digest(reader)?;
    let tree = read_digest(reader)?;
    let producer_count = bounded_len(reader, usize::from(ReviewLimits::MAX_PROVENANCE_SOURCES))?;
    let mut producers = Vec::with_capacity(producer_count);
    for _ in 0..producer_count {
        producers.push(read_actor_id(reader)?);
    }
    let ancestry_count = bounded_len(reader, usize::from(ReviewLimits::MAX_PROVENANCE_SOURCES))?;
    let mut ancestries = Vec::with_capacity(ancestry_count);
    for _ in 0..ancestry_count {
        ancestries.push(read_digest(reader)?);
    }
    let value = ReviewBinding::from_wire(
        contract_id,
        contract_digest,
        revision,
        categories,
        reviewer_quorum,
        independence,
        severity,
        maximum_cycles,
        waiver,
        candidate,
        tree,
        producers,
        ancestries,
        read_digest(reader)?,
    );
    value.validate(production_limits()).map_err(|_| invalid(reader))?;
    Ok(value)
}

fn bounded_len(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count > maximum { Err(invalid(reader)) } else { Ok(count) }
}

fn read_digests<T>(
    reader: &mut CanonicalReader<'_>,
    maximum: u16,
    construct: impl Fn(Sha256Digest) -> T,
) -> Result<Vec<T>, CodecError> {
    let count = bounded_len(reader, usize::from(maximum))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(construct(read_digest(reader)?));
    }
    Ok(values)
}

fn write_reviewer(
    writer: &mut CanonicalWriter,
    value: &ReviewerIdentity,
) -> Result<(), CodecError> {
    write_id(writer, value.actor_id().as_bytes())?;
    write_digest(writer, value.provider())?;
    write_digest(writer, value.model_family())?;
    write_digest(writer, value.prompt_revision())?;
    write_digest(writer, value.context())?;
    write_digest(writer, value.ancestry())?;
    writer.write_bool(value.independent_from_producer())
}

fn read_reviewer(reader: &mut CanonicalReader<'_>) -> Result<ReviewerIdentity, CodecError> {
    Ok(ReviewerIdentity::new(
        read_actor_id(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        reader.read_bool()?,
    ))
}
