//! Candidate, handoff, and directive canonical records.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};
use peritus_collaboration::CollaborationTaskId;
use peritus_scheduler::WorkId;
use peritus_types::{Sha256Digest, TurnId};

use crate::{
    CandidateBinding, DirectiveId, Handoff, HandoffId, OrchestratorLimits, PendingDirective,
};

use super::super::{
    destination_tag, directive_kind_tag, handoff_kind_tag, handoff_role_tag, read_actor_id,
    read_artifact_id, read_destination, read_digest, read_directive_kind, read_event_id,
    read_handoff_kind, read_handoff_role, read_revision, read_snapshot_id, write_digest, write_id,
    write_revision,
};

pub fn write_candidate(
    writer: &mut CanonicalWriter,
    value: &CandidateBinding,
) -> Result<(), CodecError> {
    write_revision(writer, value.revision())?;
    write_id(writer, value.snapshot_id().as_bytes())?;
    write_digest(writer, value.candidate_digest())?;
    write_digest(writer, value.tree_digest())?;
    write_digest(writer, value.quality_snapshot_digest())?;
    writer.write_option_tag(value.artifact_id().is_some())?;
    if let Some(id) = value.artifact_id() {
        write_id(writer, id.as_bytes())?;
    }
    writer.write_option_tag(value.artifact_digest().is_some())?;
    if let Some(digest) = value.artifact_digest() {
        write_digest(writer, digest)?;
    }
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

pub fn read_candidate(
    reader: &mut CanonicalReader<'_>,
    limits: OrchestratorLimits,
) -> Result<CandidateBinding, CodecError> {
    let revision = read_revision(reader)?;
    let snapshot = read_snapshot_id(reader)?;
    let candidate_digest = read_digest(reader)?;
    let tree_digest = read_digest(reader)?;
    let quality_snapshot_digest = read_digest(reader)?;
    let artifact = reader.read_option_tag()?.then(|| read_artifact_id(reader)).transpose()?;
    let artifact_digest = reader.read_option_tag()?.then(|| read_digest(reader)).transpose()?;
    let count = bounded_count(reader, usize::from(limits.artifact_references()))?;
    let mut actors = Vec::with_capacity(count);
    for _ in 0..count {
        actors.push(read_actor_id(reader)?);
    }
    let count = bounded_count(reader, usize::from(limits.artifact_references()))?;
    let mut ancestries = Vec::with_capacity(count);
    for _ in 0..count {
        ancestries.push(read_digest(reader)?);
    }
    let value = CandidateBinding::from_wire(
        revision,
        snapshot,
        candidate_digest,
        tree_digest,
        quality_snapshot_digest,
        artifact,
        artifact_digest,
        actors,
        ancestries,
        read_digest(reader)?,
    );
    value.validate(limits).map_err(|_| super::super::invalid(reader))?;
    Ok(value)
}

pub fn write_handoff(writer: &mut CanonicalWriter, value: &Handoff) -> Result<(), CodecError> {
    write_id(writer, value.id().as_bytes())?;
    writer.write_u8(handoff_kind_tag(value.kind()))?;
    writer.write_option_tag(value.source_phase().is_some())?;
    if let Some(phase) = value.source_phase() {
        writer.write_u8(super::super::active_phase_tag(phase))?;
    }
    writer.write_u8(handoff_role_tag(value.source_role()))?;
    writer.write_u8(super::super::active_phase_tag(value.destination_phase()))?;
    write_id(writer, value.source_actor().as_bytes())?;
    write_id(writer, value.destination_actor().as_bytes())?;
    writer.write_u8(handoff_role_tag(value.destination_role()))?;
    write_candidate(writer, value.candidate())?;
    writer.write_option_tag(value.turn_id().is_some())?;
    if let Some(id) = value.turn_id() {
        write_id(writer, id.as_bytes())?;
    }
    write_id(writer, value.task_id().as_bytes())?;
    write_id(writer, value.work_id().as_bytes())?;
    write_digests(writer, value.artifact_inputs())?;
    write_digests(writer, value.evidence_inputs())?;
    writer.write_collection_len(value.blocking_findings().len())?;
    for finding in value.blocking_findings() {
        write_id(writer, finding.as_bytes())?;
    }
    write_digest(writer, value.digest())
}

pub fn read_handoff(
    reader: &mut CanonicalReader<'_>,
    limits: OrchestratorLimits,
) -> Result<Handoff, CodecError> {
    let id = read_nominal(reader, HandoffId::new)?;
    let kind = read_handoff_kind(reader)?;
    let source_phase =
        reader.read_option_tag()?.then(|| super::super::read_active_phase(reader)).transpose()?;
    let source_role = read_handoff_role(reader)?;
    let destination_phase = super::super::read_active_phase(reader)?;
    let source_actor = read_actor_id(reader)?;
    let destination_actor = read_actor_id(reader)?;
    let destination_role = read_handoff_role(reader)?;
    let candidate = read_candidate(reader, limits)?;
    let turn = reader.read_option_tag()?.then(|| read_nominal(reader, TurnId::new)).transpose()?;
    let task = read_nominal(reader, CollaborationTaskId::new)?;
    let work = read_nominal(reader, WorkId::new)?;
    let artifacts = read_digests(reader, usize::from(limits.artifact_references()))?;
    let evidence = read_digests(reader, usize::from(limits.artifact_references()))?;
    let count = bounded_count(reader, usize::from(limits.artifact_references()))?;
    let mut findings = Vec::with_capacity(count);
    for _ in 0..count {
        findings.push(super::super::read_finding_id(reader)?);
    }
    let value = Handoff::from_wire(
        id,
        kind,
        source_phase,
        source_role,
        destination_phase,
        source_actor,
        destination_actor,
        destination_role,
        candidate,
        turn,
        task,
        work,
        artifacts,
        evidence,
        findings,
        read_digest(reader)?,
    );
    value.validate(limits).map_err(|_| super::super::invalid(reader))?;
    Ok(value)
}

pub fn write_directive(
    writer: &mut CanonicalWriter,
    value: &PendingDirective,
) -> Result<(), CodecError> {
    write_id(writer, value.id().as_bytes())?;
    writer.write_u8(destination_tag(value.destination()))?;
    writer.write_u8(directive_kind_tag(value.kind()))?;
    write_digest(writer, value.payload_digest())?;
    writer.write_u16(value.maximum_deliveries())?;
    writer.write_u16(value.deliveries())?;
    writer.write_u8(super::super::delivery_tag(value.delivery_state()))?;
    write_id(writer, value.source_event().as_bytes())?;
    writer.write_option_tag(value.task_id().is_some())?;
    if let Some(id) = value.task_id() {
        write_id(writer, id.as_bytes())?;
    }
    writer.write_option_tag(value.work_id().is_some())?;
    if let Some(id) = value.work_id() {
        write_id(writer, id.as_bytes())?;
    }
    write_revision(writer, value.revision())
}

pub fn read_directive(reader: &mut CanonicalReader<'_>) -> Result<PendingDirective, CodecError> {
    PendingDirective::from_wire(
        read_nominal(reader, DirectiveId::new)?,
        read_destination(reader)?,
        read_directive_kind(reader)?,
        read_digest(reader)?,
        reader.read_u16()?,
        reader.read_u16()?,
        super::super::read_delivery(reader)?,
        read_event_id(reader)?,
        reader
            .read_option_tag()?
            .then(|| read_nominal(reader, CollaborationTaskId::new))
            .transpose()?,
        reader.read_option_tag()?.then(|| read_nominal(reader, WorkId::new)).transpose()?,
        read_revision(reader)?,
    )
    .map_err(|_| super::super::invalid(reader))
}

fn write_digests(writer: &mut CanonicalWriter, values: &[Sha256Digest]) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        write_digest(writer, *value)?;
    }
    Ok(())
}

fn read_digests(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
) -> Result<Vec<Sha256Digest>, CodecError> {
    let count = bounded_count(reader, maximum)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(read_digest(reader)?);
    }
    Ok(values)
}

fn bounded_count(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count > maximum { Err(super::super::invalid(reader)) } else { Ok(count) }
}

fn read_nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    constructor(reader.read_fixed()?).map_err(|_| super::super::invalid_at(offset))
}
