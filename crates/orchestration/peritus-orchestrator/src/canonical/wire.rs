//! Shared closed-tag and primitive codec helpers for families 76-78.

pub mod domain;
pub mod observation;
mod records;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_policy::ActorRole;
use peritus_role::HarnessRole;
use peritus_types::{
    AcceptanceSpecId, ActorId, ArtifactId, AttemptId, CommandId, EventId, EventSequence, FindingId,
    Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple, RunId,
    Sha256Digest, SnapshotId, WorkspaceId,
};

use crate::{ActivePhase, OrchestratorPhase};

pub const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

pub const fn invalid_at(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, offset)
}

pub const fn unknown(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, offset)
}

pub fn write_id(writer: &mut CanonicalWriter, bytes: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(bytes)
}

pub fn write_digest(writer: &mut CanonicalWriter, value: Sha256Digest) -> Result<(), CodecError> {
    writer.write_fixed(value.as_bytes())
}

pub fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

fn read_nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    constructor(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn read_acceptance_id(
    reader: &mut CanonicalReader<'_>,
) -> Result<AcceptanceSpecId, CodecError> {
    read_nominal(reader, AcceptanceSpecId::new)
}
pub fn read_actor_id(reader: &mut CanonicalReader<'_>) -> Result<ActorId, CodecError> {
    read_nominal(reader, ActorId::new)
}
pub fn read_artifact_id(reader: &mut CanonicalReader<'_>) -> Result<ArtifactId, CodecError> {
    read_nominal(reader, ArtifactId::new)
}
pub fn read_attempt_id(reader: &mut CanonicalReader<'_>) -> Result<AttemptId, CodecError> {
    read_nominal(reader, AttemptId::new)
}
pub fn read_command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    read_nominal(reader, CommandId::new)
}
pub fn read_event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    read_nominal(reader, EventId::new)
}
pub fn read_finding_id(reader: &mut CanonicalReader<'_>) -> Result<FindingId, CodecError> {
    read_nominal(reader, FindingId::new)
}
pub fn read_harness_id(reader: &mut CanonicalReader<'_>) -> Result<HarnessId, CodecError> {
    read_nominal(reader, HarnessId::new)
}
pub fn read_policy_id(reader: &mut CanonicalReader<'_>) -> Result<PolicyId, CodecError> {
    read_nominal(reader, PolicyId::new)
}
pub fn read_provider_id(reader: &mut CanonicalReader<'_>) -> Result<ProviderProfileId, CodecError> {
    read_nominal(reader, ProviderProfileId::new)
}
pub fn read_run_id(reader: &mut CanonicalReader<'_>) -> Result<RunId, CodecError> {
    read_nominal(reader, RunId::new)
}
pub fn read_snapshot_id(reader: &mut CanonicalReader<'_>) -> Result<SnapshotId, CodecError> {
    read_nominal(reader, SnapshotId::new)
}
pub fn read_workspace_id(reader: &mut CanonicalReader<'_>) -> Result<WorkspaceId, CodecError> {
    read_nominal(reader, WorkspaceId::new)
}

pub fn write_revision(
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

pub fn read_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, CodecError> {
    let acceptance = read_acceptance_id(reader)?;
    let harness = read_harness_id(reader)?;
    let workspace = read_workspace_id(reader)?;
    let generation = Generation::new(reader.read_u64()?).map_err(|_| invalid(reader))?;
    let revision = RevisionNumber::new(reader.read_u64()?).map_err(|_| invalid(reader))?;
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

pub fn write_event_option(
    writer: &mut CanonicalWriter,
    value: Option<EventId>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_id(writer, value.as_bytes())?;
    }
    Ok(())
}

pub fn read_event_option(reader: &mut CanonicalReader<'_>) -> Result<Option<EventId>, CodecError> {
    reader.read_option_tag()?.then(|| read_event_id(reader)).transpose()
}

pub fn read_sequence(reader: &mut CanonicalReader<'_>) -> Result<EventSequence, CodecError> {
    EventSequence::new(reader.read_u64()?).map_err(|_| invalid(reader))
}

pub const fn active_phase_tag(value: ActivePhase) -> u8 {
    match value {
        ActivePhase::WriterPending => 1,
        ActivePhase::WriterActive => 2,
        ActivePhase::GatesPending => 3,
        ActivePhase::GatesActive => 4,
        ActivePhase::ReviewPending => 5,
        ActivePhase::ReviewActive => 6,
        ActivePhase::FixerPending => 7,
        ActivePhase::FixerActive => 8,
        ActivePhase::RevisionAdvancing => 9,
        ActivePhase::EvaluatingAcceptance => 10,
        ActivePhase::KernelAcceptancePending => 11,
    }
}

pub fn read_active_phase(reader: &mut CanonicalReader<'_>) -> Result<ActivePhase, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ActivePhase::WriterPending),
        2 => Ok(ActivePhase::WriterActive),
        3 => Ok(ActivePhase::GatesPending),
        4 => Ok(ActivePhase::GatesActive),
        5 => Ok(ActivePhase::ReviewPending),
        6 => Ok(ActivePhase::ReviewActive),
        7 => Ok(ActivePhase::FixerPending),
        8 => Ok(ActivePhase::FixerActive),
        9 => Ok(ActivePhase::RevisionAdvancing),
        10 => Ok(ActivePhase::EvaluatingAcceptance),
        11 => Ok(ActivePhase::KernelAcceptancePending),
        _ => Err(unknown(offset)),
    }
}

pub fn write_phase(
    writer: &mut CanonicalWriter,
    value: OrchestratorPhase,
) -> Result<(), CodecError> {
    match value {
        OrchestratorPhase::Active(phase) => {
            writer.write_u8(1)?;
            writer.write_u8(active_phase_tag(phase))
        }
        OrchestratorPhase::Paused(phase) => {
            writer.write_u8(2)?;
            writer.write_u8(active_phase_tag(phase))
        }
        OrchestratorPhase::Cancelling => writer.write_u8(3),
        OrchestratorPhase::Terminal => writer.write_u8(4),
    }
}

pub fn read_phase(reader: &mut CanonicalReader<'_>) -> Result<OrchestratorPhase, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => read_active_phase(reader).map(OrchestratorPhase::Active),
        2 => read_active_phase(reader).map(OrchestratorPhase::Paused),
        3 => Ok(OrchestratorPhase::Cancelling),
        4 => Ok(OrchestratorPhase::Terminal),
        _ => Err(unknown(offset)),
    }
}

pub const fn harness_role_tag(value: HarnessRole) -> u8 {
    match value {
        HarnessRole::Writer => 1,
        HarnessRole::Reviewer => 2,
        HarnessRole::Fixer => 3,
        HarnessRole::Evaluator => 4,
        HarnessRole::Evolver => 5,
    }
}

pub fn read_harness_role(reader: &mut CanonicalReader<'_>) -> Result<HarnessRole, CodecError> {
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

pub const fn actor_role_tag(value: ActorRole) -> u8 {
    match value {
        ActorRole::Writer => 1,
        ActorRole::Fixer => 2,
        ActorRole::Reviewer => 3,
        ActorRole::Evaluator => 4,
        ActorRole::GateRunner => 5,
        ActorRole::Orchestrator => 6,
        ActorRole::EvolutionAgent => 7,
        ActorRole::HumanAuthority => 8,
        ActorRole::DaemonService => 9,
        ActorRole::ProviderToolWorker => 10,
        ActorRole::Plugin => 11,
    }
}

pub fn read_actor_role(reader: &mut CanonicalReader<'_>) -> Result<ActorRole, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ActorRole::Writer),
        2 => Ok(ActorRole::Fixer),
        3 => Ok(ActorRole::Reviewer),
        4 => Ok(ActorRole::Evaluator),
        5 => Ok(ActorRole::GateRunner),
        6 => Ok(ActorRole::Orchestrator),
        7 => Ok(ActorRole::EvolutionAgent),
        8 => Ok(ActorRole::HumanAuthority),
        9 => Ok(ActorRole::DaemonService),
        10 => Ok(ActorRole::ProviderToolWorker),
        11 => Ok(ActorRole::Plugin),
        _ => Err(unknown(offset)),
    }
}

mod tags;
pub use tags::*;
