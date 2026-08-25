//! Collaboration-owned typed B3 codecs for canonical families 73, 74, and 75.

mod command;
mod event;
mod state;

#[cfg(test)]
mod fixture_tests;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_role::HarnessRole;
use peritus_scheduler::{DispatchId, SchedulerId, WorkId};
use peritus_types::{
    AcceptanceSpecId, ActorId, ArtifactId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use crate::{
    ArtifactHandoff, CollaborationBinding, CollaborationId, CollaborationLimits,
    CollaborationMessage, CollaborationMessageId, CollaborationTaskId, Delegation, JoinPolicy,
    ReservationObservation, TaskTerminal, TaskTerminalKind,
};

pub use command::CollaborationCommandFrame;
pub use event::CollaborationEventFrame;
pub use state::CollaborationStateFrame;

const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

const fn unknown(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, offset)
}

fn write_id(writer: &mut CanonicalWriter, value: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(value)
}

fn read_nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    construct: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    construct(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

fn read_collaboration_id(reader: &mut CanonicalReader<'_>) -> Result<CollaborationId, CodecError> {
    read_nominal(reader, CollaborationId::new)
}
fn read_task_id(reader: &mut CanonicalReader<'_>) -> Result<CollaborationTaskId, CodecError> {
    read_nominal(reader, CollaborationTaskId::new)
}
fn read_message_id(reader: &mut CanonicalReader<'_>) -> Result<CollaborationMessageId, CodecError> {
    read_nominal(reader, CollaborationMessageId::new)
}
fn read_scheduler_id(reader: &mut CanonicalReader<'_>) -> Result<SchedulerId, CodecError> {
    read_nominal(reader, SchedulerId::new)
}
fn read_work_id(reader: &mut CanonicalReader<'_>) -> Result<WorkId, CodecError> {
    read_nominal(reader, WorkId::new)
}
fn read_dispatch_id(reader: &mut CanonicalReader<'_>) -> Result<DispatchId, CodecError> {
    read_nominal(reader, DispatchId::new)
}
fn read_actor_id(reader: &mut CanonicalReader<'_>) -> Result<ActorId, CodecError> {
    read_nominal(reader, ActorId::new)
}
fn read_artifact_id(reader: &mut CanonicalReader<'_>) -> Result<ArtifactId, CodecError> {
    read_nominal(reader, ArtifactId::new)
}
fn read_command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    read_nominal(reader, CommandId::new)
}
fn read_event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    read_nominal(reader, EventId::new)
}
fn read_run_id(reader: &mut CanonicalReader<'_>) -> Result<RunId, CodecError> {
    read_nominal(reader, RunId::new)
}
fn read_acceptance_id(reader: &mut CanonicalReader<'_>) -> Result<AcceptanceSpecId, CodecError> {
    read_nominal(reader, AcceptanceSpecId::new)
}
fn read_harness_id(reader: &mut CanonicalReader<'_>) -> Result<HarnessId, CodecError> {
    read_nominal(reader, HarnessId::new)
}
fn read_workspace_id(reader: &mut CanonicalReader<'_>) -> Result<WorkspaceId, CodecError> {
    read_nominal(reader, WorkspaceId::new)
}
fn read_policy_id(reader: &mut CanonicalReader<'_>) -> Result<PolicyId, CodecError> {
    read_nominal(reader, PolicyId::new)
}
fn read_provider_id(reader: &mut CanonicalReader<'_>) -> Result<ProviderProfileId, CodecError> {
    read_nominal(reader, ProviderProfileId::new)
}

fn write_digest(writer: &mut CanonicalWriter, digest: Sha256Digest) -> Result<(), CodecError> {
    writer.write_fixed(digest.as_bytes())
}
fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

fn write_revision(writer: &mut CanonicalWriter, value: RevisionTuple) -> Result<(), CodecError> {
    write_id(writer, value.acceptance_spec_id().as_bytes())?;
    write_id(writer, value.harness_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    writer.write_u64(value.workspace_generation().get())?;
    writer.write_u64(value.workspace_revision().get())?;
    write_id(writer, value.policy_id().as_bytes())?;
    write_id(writer, value.provider_profile_id().as_bytes())
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
        read_provider_id(reader)?,
    ))
}

fn write_limits(
    writer: &mut CanonicalWriter,
    value: CollaborationLimits,
) -> Result<(), CodecError> {
    writer.write_u32(value.tasks())?;
    writer.write_u16(value.depth())?;
    writer.write_u16(value.fan_out())?;
    writer.write_u32(value.messages())?;
    writer.write_u16(value.recipients())?;
    writer.write_u32(value.payload_bytes())?;
    writer.write_u16(value.artifact_references())?;
    writer.write_u64(value.command_bytes())?;
    writer.write_u64(value.state_bytes())
}

fn read_limits(reader: &mut CanonicalReader<'_>) -> Result<CollaborationLimits, CodecError> {
    let offset = reader.offset();
    CollaborationLimits::new(
        reader.read_u32()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u32()?,
        reader.read_u16()?,
        reader.read_u32()?,
        reader.read_u16()?,
        reader.read_u64()?,
        reader.read_u64()?,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

fn write_binding(
    writer: &mut CanonicalWriter,
    value: &CollaborationBinding,
) -> Result<(), CodecError> {
    write_id(writer, value.id().as_bytes())?;
    write_id(writer, value.run_id().as_bytes())?;
    write_revision(writer, value.revision())?;
    write_id(writer, value.scheduler_id().as_bytes())?;
    write_id(writer, value.root_task_id().as_bytes())?;
    write_limits(writer, value.limits())?;
    write_delegation(writer, value.root_assignment())?;
    write_digest(writer, value.digest())
}

fn read_binding(reader: &mut CanonicalReader<'_>) -> Result<CollaborationBinding, CodecError> {
    let binding = CollaborationBinding::from_wire(
        read_collaboration_id(reader)?,
        read_run_id(reader)?,
        read_revision(reader)?,
        read_scheduler_id(reader)?,
        read_task_id(reader)?,
        read_limits(reader)?,
        read_delegation(reader)?,
        read_digest(reader)?,
    );
    binding.validate().map_err(|_| invalid(reader))?;
    Ok(binding)
}

fn write_delegation(writer: &mut CanonicalWriter, value: &Delegation) -> Result<(), CodecError> {
    write_id(writer, value.task_id().as_bytes())?;
    write_id(writer, value.root_task_id().as_bytes())?;
    writer.write_option_tag(value.parent_task_id().is_some())?;
    if let Some(parent) = value.parent_task_id() {
        write_id(writer, parent.as_bytes())?;
    }
    writer.write_u16(value.depth())?;
    write_id(writer, value.owner().as_bytes())?;
    writer.write_u8(crate::canonical::role_tag(value.role()))?;
    write_id(writer, value.parent_owner().as_bytes())?;
    write_id(writer, value.work_id().as_bytes())?;
    write_digest(writer, value.goal_digest())?;
    writer.write_bool(value.required())?;
    writer.write_u8(crate::canonical::join_tag(value.join_policy()))
}

fn read_delegation(reader: &mut CanonicalReader<'_>) -> Result<Delegation, CodecError> {
    let task_id = read_task_id(reader)?;
    let root_id = read_task_id(reader)?;
    let parent = reader.read_option_tag()?.then(|| read_task_id(reader)).transpose()?;
    let depth = reader.read_u16()?;
    let owner = read_actor_id(reader)?;
    let role = read_role(reader)?;
    let parent_owner = read_actor_id(reader)?;
    let work_id = read_work_id(reader)?;
    let goal = read_digest(reader)?;
    let required = reader.read_bool()?;
    let join = read_join(reader)?;
    let result = if let Some(parent) = parent {
        Delegation::child(
            task_id,
            root_id,
            parent,
            depth,
            owner,
            role,
            parent_owner,
            work_id,
            goal,
            required,
            join,
        )
    } else if required {
        Delegation::root(task_id, owner, role, work_id, goal, join)
    } else {
        return Err(invalid(reader));
    };
    result.map_err(|_| invalid(reader))
}

fn read_role(reader: &mut CanonicalReader<'_>) -> Result<HarnessRole, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(HarnessRole::Writer),
        2 => Ok(HarnessRole::Reviewer),
        3 => Ok(HarnessRole::Fixer),
        4 => Ok(HarnessRole::Evaluator),
        5 => Ok(HarnessRole::Evolver),
        _ => Err(unknown(offset)),
    }
}

fn read_join(reader: &mut CanonicalReader<'_>) -> Result<JoinPolicy, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(JoinPolicy::NoChildren),
        2 => Ok(JoinPolicy::AllRequired),
        3 => Ok(JoinPolicy::AnyRequired),
        _ => Err(unknown(offset)),
    }
}

fn write_reservation(
    writer: &mut CanonicalWriter,
    value: ReservationObservation,
) -> Result<(), CodecError> {
    write_id(writer, value.work_id().as_bytes())?;
    write_id(writer, value.dispatch_id().as_bytes())?;
    write_id(writer, value.owner().as_bytes())?;
    write_revision(writer, value.revision())
}

fn read_reservation(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReservationObservation, CodecError> {
    Ok(ReservationObservation::new(
        read_work_id(reader)?,
        read_dispatch_id(reader)?,
        read_actor_id(reader)?,
        read_revision(reader)?,
    ))
}

fn write_artifact(writer: &mut CanonicalWriter, value: ArtifactHandoff) -> Result<(), CodecError> {
    write_id(writer, value.artifact_id().as_bytes())?;
    write_digest(writer, value.artifact_digest())?;
    write_digest(writer, value.evidence_digest())?;
    write_revision(writer, value.revision())
}

fn read_artifact(reader: &mut CanonicalReader<'_>) -> Result<ArtifactHandoff, CodecError> {
    ArtifactHandoff::new(
        read_artifact_id(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_revision(reader)?,
    )
    .map_err(|_| invalid(reader))
}

fn write_task_terminal(
    writer: &mut CanonicalWriter,
    value: TaskTerminal,
) -> Result<(), CodecError> {
    writer.write_u8(crate::canonical::task_terminal_tag(value.kind()))?;
    writer.write_option_tag(value.handoff().is_some())?;
    if let Some(handoff) = value.handoff() {
        write_artifact(writer, handoff)?;
    }
    write_digest(writer, value.cause_digest())
}

fn read_task_terminal(reader: &mut CanonicalReader<'_>) -> Result<TaskTerminal, CodecError> {
    let offset = reader.offset();
    let kind = match reader.read_u8()? {
        1 => TaskTerminalKind::Succeeded,
        2 => TaskTerminalKind::Failed,
        3 => TaskTerminalKind::Rejected,
        4 => TaskTerminalKind::Cancelled,
        5 => TaskTerminalKind::Abandoned,
        _ => return Err(unknown(offset)),
    };
    let handoff = reader.read_option_tag()?.then(|| read_artifact(reader)).transpose()?;
    TaskTerminal::new(kind, handoff, read_digest(reader)?).map_err(|_| invalid(reader))
}

fn write_message(
    writer: &mut CanonicalWriter,
    value: &CollaborationMessage,
) -> Result<(), CodecError> {
    write_id(writer, value.id().as_bytes())?;
    write_id(writer, value.root_task_id().as_bytes())?;
    write_id(writer, value.task_id().as_bytes())?;
    write_id(writer, value.sender().as_bytes())?;
    write_id(writer, value.receiver().as_bytes())?;
    writer.write_u32(value.ordinal())?;
    writer.write_option_tag(value.predecessor().is_some())?;
    if let Some(predecessor) = value.predecessor() {
        write_id(writer, predecessor.as_bytes())?;
    }
    writer.write_str(value.media_type())?;
    writer.write_u32(value.payload_bytes())?;
    write_digest(writer, value.content_digest())?;
    writer.write_option_tag(value.artifact().is_some())?;
    if let Some(artifact) = value.artifact() {
        write_artifact(writer, artifact)?;
    }
    write_revision(writer, value.revision())
}

fn read_message(reader: &mut CanonicalReader<'_>) -> Result<CollaborationMessage, CodecError> {
    let id = read_message_id(reader)?;
    let root = read_task_id(reader)?;
    let task = read_task_id(reader)?;
    let sender = read_actor_id(reader)?;
    let receiver = read_actor_id(reader)?;
    let ordinal = reader.read_u32()?;
    let predecessor = reader.read_option_tag()?.then(|| read_message_id(reader)).transpose()?;
    let media_type = reader.read_str()?.to_owned();
    let payload_bytes = reader.read_u32()?;
    let digest = read_digest(reader)?;
    let artifact = reader.read_option_tag()?.then(|| read_artifact(reader)).transpose()?;
    CollaborationMessage::new(
        id,
        root,
        task,
        sender,
        receiver,
        ordinal,
        predecessor,
        media_type,
        payload_bytes,
        digest,
        artifact,
        read_revision(reader)?,
    )
    .map_err(|_| invalid(reader))
}

fn bounded_len(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let offset = reader.offset();
    let count = reader.read_collection_len()?;
    if count > maximum {
        Err(CodecError::at(CodecErrorKind::LimitExceeded, offset))
    } else {
        Ok(count)
    }
}
