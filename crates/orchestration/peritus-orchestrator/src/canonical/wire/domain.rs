//! Canonical codecs for immutable E0 domain records.

mod material;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};
use peritus_collaboration::CollaborationId;
use peritus_scheduler::SchedulerId;

use crate::{
    OrchestratorBinding, OrchestratorId, OrchestratorLimits, QualityCycleBinding,
    ResumeReconciliation, RoleAssignment, RoleOwnership,
};

use super::{
    actor_role_tag, harness_role_tag, read_actor_id, read_actor_role, read_digest,
    read_harness_role, read_revision, read_run_id, write_digest, write_id, write_revision,
};

pub use super::records::{read_certificate, read_terminal, write_certificate, write_terminal};
pub use material::{
    read_candidate, read_directive, read_handoff, write_candidate, write_directive, write_handoff,
};

pub const fn wire_limits() -> OrchestratorLimits {
    OrchestratorLimits::from_wire(
        OrchestratorLimits::MAX_REVISIONS,
        OrchestratorLimits::MAX_WRITER_CYCLES,
        OrchestratorLimits::MAX_FIXER_CYCLES,
        OrchestratorLimits::MAX_GATE_CYCLES,
        OrchestratorLimits::MAX_REVIEW_CYCLES,
        OrchestratorLimits::MAX_HANDOFFS,
        OrchestratorLimits::MAX_CHILD_DIRECTIVES,
        OrchestratorLimits::MAX_RETAINED_OBSERVATIONS,
        OrchestratorLimits::MAX_ARTIFACT_REFERENCES,
        OrchestratorLimits::MAX_CANCELLATION_RECONCILIATIONS,
        OrchestratorLimits::MAX_EVENT_BYTES,
        OrchestratorLimits::MAX_STATE_BYTES,
    )
}

pub fn write_limits(
    writer: &mut CanonicalWriter,
    value: OrchestratorLimits,
) -> Result<(), CodecError> {
    for item in [
        value.revisions(),
        value.writer_cycles(),
        value.fixer_cycles(),
        value.gate_cycles(),
        value.review_cycles(),
        value.handoffs(),
        value.child_directives(),
        value.retained_observations(),
        value.artifact_references(),
        value.cancellation_reconciliations(),
    ] {
        writer.write_u16(item)?;
    }
    writer.write_u64(value.event_bytes())?;
    writer.write_u64(value.state_bytes())
}

pub fn read_limits(reader: &mut CanonicalReader<'_>) -> Result<OrchestratorLimits, CodecError> {
    let offset = reader.offset();
    OrchestratorLimits::new(
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u64()?,
        reader.read_u64()?,
    )
    .map_err(|_| super::invalid_at(offset))
}

pub fn write_binding(
    writer: &mut CanonicalWriter,
    value: &OrchestratorBinding,
) -> Result<(), CodecError> {
    write_id(writer, value.id().as_bytes())?;
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.attempt_id().as_bytes())?;
    write_id(writer, value.contract_id().as_bytes())?;
    write_digest(writer, value.contract_digest())?;
    write_revision(writer, value.initial_revision())?;
    write_id(writer, value.initial_gate_run_id().as_bytes())?;
    write_id(writer, value.initial_scheduler_run_id().as_bytes())?;
    write_id(writer, value.initial_collaboration_run_id().as_bytes())?;
    writer.write_u16(value.contract_gate_cycles())?;
    writer.write_u16(value.contract_review_cycles())?;
    write_digest(writer, value.gate_plan_digest())?;
    write_digest(writer, value.review_binding_digest())?;
    write_id(writer, value.scheduler_id().as_bytes())?;
    write_digest(writer, value.scheduler_binding_digest())?;
    write_id(writer, value.collaboration_id().as_bytes())?;
    write_digest(writer, value.collaboration_binding_digest())?;
    write_limits(writer, value.limits())?;
    write_digest(writer, value.digest())
}

pub fn read_binding(reader: &mut CanonicalReader<'_>) -> Result<OrchestratorBinding, CodecError> {
    let value = OrchestratorBinding::from_wire(
        read_nominal(reader, OrchestratorId::new)?,
        read_run_id(reader)?,
        super::read_attempt_id(reader)?,
        super::read_acceptance_id(reader)?,
        read_digest(reader)?,
        read_revision(reader)?,
        read_run_id(reader)?,
        read_run_id(reader)?,
        read_run_id(reader)?,
        reader.read_u16()?,
        reader.read_u16()?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_nominal(reader, SchedulerId::new)?,
        read_digest(reader)?,
        read_nominal(reader, CollaborationId::new)?,
        read_digest(reader)?,
        read_limits(reader)?,
        read_digest(reader)?,
    );
    value.validate().map_err(|_| super::invalid(reader))?;
    Ok(value)
}

pub fn write_quality_cycle(
    writer: &mut CanonicalWriter,
    value: &QualityCycleBinding,
) -> Result<(), CodecError> {
    write_revision(writer, value.revision())?;
    write_id(writer, value.gate_run_id().as_bytes())?;
    write_id(writer, value.scheduler_run_id().as_bytes())?;
    write_id(writer, value.collaboration_run_id().as_bytes())?;
    write_digest(writer, value.gate_plan_digest())?;
    write_digest(writer, value.review_binding_digest())?;
    write_id(writer, value.scheduler_id().as_bytes())?;
    write_digest(writer, value.scheduler_binding_digest())?;
    write_id(writer, value.collaboration_id().as_bytes())?;
    write_digest(writer, value.collaboration_binding_digest())?;
    write_digest(writer, value.digest())
}

pub fn read_quality_cycle(
    reader: &mut CanonicalReader<'_>,
) -> Result<QualityCycleBinding, CodecError> {
    let value = QualityCycleBinding::from_wire(
        read_revision(reader)?,
        read_run_id(reader)?,
        read_run_id(reader)?,
        read_run_id(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_nominal(reader, SchedulerId::new)?,
        read_digest(reader)?,
        read_nominal(reader, CollaborationId::new)?,
        read_digest(reader)?,
        read_digest(reader)?,
    );
    value.validate().map_err(|_| super::invalid(reader))?;
    Ok(value)
}

pub fn write_reconciliation(
    writer: &mut CanonicalWriter,
    value: &ResumeReconciliation,
) -> Result<(), CodecError> {
    write_digest(writer, value.checkpoint_state_digest())?;
    writer.write_collection_len(value.child_heads().len())?;
    for head in value.child_heads() {
        super::observation::write_head(writer, *head)?;
    }
    Ok(())
}

pub fn read_reconciliation(
    reader: &mut CanonicalReader<'_>,
) -> Result<ResumeReconciliation, CodecError> {
    let digest = read_digest(reader)?;
    let count = bounded_count(reader, 6)?;
    let mut heads = Vec::with_capacity(count);
    for _ in 0..count {
        heads.push(super::observation::read_head(reader)?);
    }
    ResumeReconciliation::from_wire(digest, heads).map_err(|_| super::invalid(reader))
}

fn write_assignment(writer: &mut CanonicalWriter, value: RoleAssignment) -> Result<(), CodecError> {
    write_id(writer, value.actor().as_bytes())?;
    writer.write_u8(actor_role_tag(value.actor_role()))?;
    writer.write_u8(harness_role_tag(value.harness_role()))
}

fn read_assignment(reader: &mut CanonicalReader<'_>) -> Result<RoleAssignment, CodecError> {
    let value = RoleAssignment::from_wire(
        read_actor_id(reader)?,
        read_actor_role(reader)?,
        read_harness_role(reader)?,
    );
    value.validate().map_err(|_| super::invalid(reader))?;
    Ok(value)
}

pub fn write_ownership(
    writer: &mut CanonicalWriter,
    value: &RoleOwnership,
) -> Result<(), CodecError> {
    write_id(writer, value.service_actor().as_bytes())?;
    writer.write_u8(actor_role_tag(value.service_role()))?;
    write_assignment(writer, value.writer())?;
    write_assignment(writer, value.fixer())?;
    writer.write_collection_len(value.reviewers().len())?;
    for reviewer in value.reviewers() {
        write_assignment(writer, *reviewer)?;
    }
    Ok(())
}

pub fn read_ownership(
    reader: &mut CanonicalReader<'_>,
    limits: OrchestratorLimits,
) -> Result<RoleOwnership, CodecError> {
    let service_actor = read_actor_id(reader)?;
    let service_role = read_actor_role(reader)?;
    let writer = read_assignment(reader)?;
    let fixer = read_assignment(reader)?;
    let count = bounded_count(reader, usize::from(limits.child_directives()))?;
    let mut reviewers = Vec::with_capacity(count);
    for _ in 0..count {
        reviewers.push(read_assignment(reader)?);
    }
    RoleOwnership::new(service_actor, service_role, writer, fixer, reviewers, limits)
        .map_err(|_| super::invalid(reader))
}

fn bounded_count(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count > maximum { Err(super::invalid(reader)) } else { Ok(count) }
}

fn read_nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    constructor(reader.read_fixed()?).map_err(|_| super::invalid_at(offset))
}
