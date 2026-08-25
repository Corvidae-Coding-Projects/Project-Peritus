//! D1-owned typed B3 codecs for reserved families 50, 51, and 52.

mod command;
mod event;
mod state;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_quality_policy::GateAttemptOrdinal;
use peritus_spec::EvidenceRequirementId;
use peritus_types::{
    AcceptanceSpecId, ActionId, CommandId, EventId, EvidenceId, GateExecutionId, GateId,
    Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId, RevisionNumber, RevisionTuple,
    RunId, Sha256Digest, WorkspaceId,
};

use crate::{
    ActiveAttempt, GateArtifact, GateAttemptResult, GateEvidenceReceipt, GateOutcomeKind,
    PublishedGateEvidence, RecoveryRequirement, RetryPermission,
};

pub use command::GateCommandFrame;
pub use event::GateEventFrame;
pub use state::GateStateFrame;

pub fn write_id(writer: &mut CanonicalWriter, bytes: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(bytes)
}

pub fn write_digest(writer: &mut CanonicalWriter, digest: Sha256Digest) -> Result<(), CodecError> {
    writer.write_fixed(digest.as_bytes())
}

pub fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}

pub fn read_action_id(reader: &mut CanonicalReader<'_>) -> Result<ActionId, CodecError> {
    read_nominal(reader, ActionId::new)
}

pub fn read_command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    read_nominal(reader, CommandId::new)
}

pub fn read_event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    read_nominal(reader, EventId::new)
}

pub fn read_evidence_id(reader: &mut CanonicalReader<'_>) -> Result<EvidenceId, CodecError> {
    read_nominal(reader, EvidenceId::new)
}

pub fn read_execution_id(reader: &mut CanonicalReader<'_>) -> Result<GateExecutionId, CodecError> {
    read_nominal(reader, GateExecutionId::new)
}

pub fn read_gate_id(reader: &mut CanonicalReader<'_>) -> Result<GateId, CodecError> {
    read_nominal(reader, GateId::new)
}

pub fn read_process_id(reader: &mut CanonicalReader<'_>) -> Result<ProcessId, CodecError> {
    read_nominal(reader, ProcessId::new)
}

pub fn read_run_id(reader: &mut CanonicalReader<'_>) -> Result<RunId, CodecError> {
    read_nominal(reader, RunId::new)
}

pub fn write_option_id(
    writer: &mut CanonicalWriter,
    value: Option<EventId>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_id(writer, value.as_bytes())?;
    }
    Ok(())
}

pub fn read_option_event(reader: &mut CanonicalReader<'_>) -> Result<Option<EventId>, CodecError> {
    reader.read_option_tag()?.then(|| read_event_id(reader)).transpose()
}

pub fn write_option_digest(
    writer: &mut CanonicalWriter,
    value: Option<Sha256Digest>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_digest(writer, value)?;
    }
    Ok(())
}

pub fn read_option_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<Sha256Digest>, CodecError> {
    reader.read_option_tag()?.then(|| read_digest(reader)).transpose()
}

pub fn write_revision(
    writer: &mut CanonicalWriter,
    revision: RevisionTuple,
) -> Result<(), CodecError> {
    write_id(writer, revision.acceptance_spec_id().as_bytes())?;
    write_id(writer, revision.harness_id().as_bytes())?;
    write_id(writer, revision.workspace_id().as_bytes())?;
    writer.write_u64(revision.workspace_generation().get())?;
    writer.write_u64(revision.workspace_revision().get())?;
    write_id(writer, revision.policy_id().as_bytes())?;
    write_id(writer, revision.provider_profile_id().as_bytes())
}

pub fn read_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, CodecError> {
    let acceptance = read_nominal(reader, AcceptanceSpecId::new)?;
    let harness = read_nominal(reader, HarnessId::new)?;
    let workspace = read_nominal(reader, WorkspaceId::new)?;
    let generation_offset = reader.offset();
    let generation = Generation::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, generation_offset))?;
    let revision_offset = reader.offset();
    let revision = RevisionNumber::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, revision_offset))?;
    let policy = read_nominal(reader, PolicyId::new)?;
    let provider = read_nominal(reader, ProviderProfileId::new)?;
    Ok(RevisionTuple::new(acceptance, harness, workspace, generation, revision, policy, provider))
}

fn read_nominal<T>(
    reader: &mut CanonicalReader<'_>,
    construct: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    construct(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn write_attempt(
    writer: &mut CanonicalWriter,
    attempt: ActiveAttempt,
) -> Result<(), CodecError> {
    write_id(writer, attempt.execution_id().as_bytes())?;
    writer.write_u16(attempt.ordinal().get())?;
    write_id(writer, attempt.action_id().as_bytes())?;
    write_digest(writer, attempt.prepared_digest())?;
    write_digest(writer, attempt.replay_digest())?;
    write_digest(writer, attempt.snapshot_digest())
}

pub fn read_attempt(reader: &mut CanonicalReader<'_>) -> Result<ActiveAttempt, CodecError> {
    let execution = read_execution_id(reader)?;
    let ordinal_offset = reader.offset();
    let ordinal = GateAttemptOrdinal::new(reader.read_u16()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, ordinal_offset))?;
    Ok(ActiveAttempt::new(
        execution,
        ordinal,
        read_action_id(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
        read_digest(reader)?,
    ))
}

pub fn write_result(
    writer: &mut CanonicalWriter,
    result: &GateAttemptResult,
) -> Result<(), CodecError> {
    write_id(writer, result.gate_id().as_bytes())?;
    writer.write_u8(crate::canonical::outcome_tag(result.kind()))?;
    write_digest(writer, result.tool_result_digest())?;
    write_option_digest(writer, result.candidate_result_digest())?;
    write_option_digest(writer, result.execution_plan_digest())?;
    writer.write_option_tag(result.process_id().is_some())?;
    if let Some(process) = result.process_id() {
        write_id(writer, process.as_bytes())?;
    }
    writer.write_u8(crate::canonical::retry_tag(result.retry_permission()))?;
    writer.write_u8(crate::canonical::recovery_tag(result.recovery_requirement()))?;
    writer.write_collection_len(result.artifacts().len())?;
    for artifact in result.artifacts() {
        write_artifact(writer, artifact)?;
    }
    Ok(())
}

pub fn read_result(reader: &mut CanonicalReader<'_>) -> Result<GateAttemptResult, CodecError> {
    let gate = read_gate_id(reader)?;
    let kind_offset = reader.offset();
    let kind = match reader.read_u8()? {
        1 => GateOutcomeKind::Passed,
        2 => GateOutcomeKind::CandidateFailure,
        3 => GateOutcomeKind::InfrastructureFailure,
        4 => GateOutcomeKind::Cancelled,
        5 => GateOutcomeKind::TimedOut,
        6 => GateOutcomeKind::MalformedOutput,
        7 => GateOutcomeKind::IncompleteEvidence,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, kind_offset)),
    };
    let tool = read_digest(reader)?;
    let candidate = read_option_digest(reader)?;
    let plan = read_option_digest(reader)?;
    let process = reader.read_option_tag()?.then(|| read_process_id(reader)).transpose()?;
    let retry_offset = reader.offset();
    let retry = match reader.read_u8()? {
        1 => RetryPermission::Never,
        2 => RetryPermission::FreshAction,
        3 => RetryPermission::AfterRecovery,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, retry_offset)),
    };
    let recovery_offset = reader.offset();
    let recovery = match reader.read_u8()? {
        1 => RecoveryRequirement::None,
        2 => RecoveryRequirement::Reauthorize,
        3 => RecoveryRequirement::ReconcileWorkspace,
        4 => RecoveryRequirement::ReconcileProcess,
        5 => RecoveryRequirement::RepublishArtifact,
        6 => RecoveryRequirement::HumanReview,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, recovery_offset)),
    };
    let count = reader.read_collection_len()?;
    let mut artifacts = Vec::with_capacity(count);
    for _ in 0..count {
        artifacts.push(read_artifact(reader)?);
    }
    GateAttemptResult::from_parts(
        gate, kind, tool, candidate, plan, process, artifacts, retry, recovery,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, kind_offset))
}

pub fn write_artifact(
    writer: &mut CanonicalWriter,
    artifact: &GateArtifact,
) -> Result<(), CodecError> {
    write_digest(writer, artifact.digest())?;
    writer.write_u64(artifact.size())?;
    writer.write_str(artifact.media_type())?;
    writer.write_str(artifact.label())
}

pub fn read_artifact(reader: &mut CanonicalReader<'_>) -> Result<GateArtifact, CodecError> {
    GateArtifact::from_parts(
        read_digest(reader)?,
        reader.read_u64()?,
        reader.read_str()?.to_owned(),
        reader.read_str()?.to_owned(),
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()))
}

pub fn write_receipt(
    writer: &mut CanonicalWriter,
    receipt: &GateEvidenceReceipt,
) -> Result<(), CodecError> {
    write_id(writer, receipt.run_id().as_bytes())?;
    write_id(writer, receipt.gate_id().as_bytes())?;
    write_attempt(writer, receipt.attempt())?;
    write_revision(writer, receipt.revision())?;
    write_id(writer, receipt.result_event().as_bytes())?;
    writer.write_u64(receipt.result_position())?;
    write_digest(writer, receipt.result_digest())?;
    writer.write_collection_len(receipt.publication().required().len())?;
    for requirement in receipt.publication().required() {
        write_digest(writer, requirement.digest())?;
    }
    writer.write_collection_len(receipt.quality_artifacts().len())?;
    for artifact in receipt.quality_artifacts() {
        write_artifact(writer, artifact)?;
    }
    write_digest(writer, receipt.manifest_digest())?;
    writer.write_collection_len(receipt.evidence().len())?;
    for item in receipt.evidence() {
        write_digest(writer, item.requirement_id().digest())?;
        write_id(writer, item.evidence_id().as_bytes())?;
        write_digest(writer, item.record_digest())?;
        writer.write_u64(item.journal_position())?;
        write_id(writer, item.producing_event().as_bytes())?;
    }
    write_digest(writer, receipt.receipt_digest())
}

pub fn read_receipt(reader: &mut CanonicalReader<'_>) -> Result<GateEvidenceReceipt, CodecError> {
    let offset = reader.offset();
    let run_id = read_run_id(reader)?;
    let gate = read_gate_id(reader)?;
    let attempt = read_attempt(reader)?;
    let revision = read_revision(reader)?;
    let result_event = read_event_id(reader)?;
    let result_position = reader.read_u64()?;
    let result_digest = read_digest(reader)?;
    let required_count = reader.read_collection_len()?;
    if required_count > crate::evidence::MAX_PUBLISHED_GATE_EVIDENCE {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()));
    }
    let mut required = Vec::with_capacity(required_count);
    for _ in 0..required_count {
        required.push(EvidenceRequirementId::new(read_digest(reader)?));
    }
    let artifact_count = reader.read_collection_len()?;
    if artifact_count > crate::outcome::MAX_GATE_ARTIFACTS {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()));
    }
    let mut artifacts = Vec::with_capacity(artifact_count);
    for _ in 0..artifact_count {
        artifacts.push(read_artifact(reader)?);
    }
    let manifest = read_digest(reader)?;
    let count = reader.read_collection_len()?;
    if count > crate::evidence::MAX_PUBLISHED_GATE_EVIDENCE {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset()));
    }
    let mut evidence = Vec::with_capacity(count);
    for _ in 0..count {
        evidence.push(PublishedGateEvidence::from_parts(
            EvidenceRequirementId::new(read_digest(reader)?),
            read_evidence_id(reader)?,
            read_digest(reader)?,
            reader.read_u64()?,
            read_event_id(reader)?,
        ));
    }
    let digest = read_digest(reader)?;
    GateEvidenceReceipt::from_wire(
        run_id,
        gate,
        attempt,
        revision,
        result_event,
        result_position,
        result_digest,
        required,
        artifacts,
        manifest,
        evidence,
        digest,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
