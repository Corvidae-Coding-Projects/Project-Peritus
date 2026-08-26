//! Exact primitive and cross-slice identity codecs shared by F0 wire families.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_harness::domain::{
    ComponentId, ComponentKind, HarnessRevisionIdentity, RevisionDigest,
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProjectId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use crate::{
    EvolutionCampaignId, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
};

pub(super) fn digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, EvolutionError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec)?))
}

pub(super) fn write_revision(
    writer: &mut CanonicalWriter,
    revision: RevisionTuple,
) -> Result<(), EvolutionError> {
    for value in [
        revision.acceptance_spec_id().as_bytes(),
        revision.harness_id().as_bytes(),
        revision.workspace_id().as_bytes(),
    ] {
        writer.write_fixed(value).map_err(codec)?;
    }
    writer.write_u64(revision.workspace_generation().get()).map_err(codec)?;
    writer.write_u64(revision.workspace_revision().get()).map_err(codec)?;
    writer.write_fixed(revision.policy_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(revision.provider_profile_id().as_bytes()).map_err(codec)
}

pub(super) fn revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, EvolutionError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(reader.read_fixed().map_err(codec)?).map_err(domain)?,
        HarnessId::new(reader.read_fixed().map_err(codec)?).map_err(domain)?,
        WorkspaceId::new(reader.read_fixed().map_err(codec)?).map_err(domain)?,
        Generation::new(reader.read_u64().map_err(codec)?).map_err(domain)?,
        RevisionNumber::new(reader.read_u64().map_err(codec)?).map_err(domain)?,
        PolicyId::new(reader.read_fixed().map_err(codec)?).map_err(domain)?,
        ProviderProfileId::new(reader.read_fixed().map_err(codec)?).map_err(domain)?,
    ))
}

pub(super) fn write_harness_revision(
    writer: &mut CanonicalWriter,
    value: HarnessRevisionIdentity,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.harness_id().as_bytes()).map_err(codec)?;
    writer.write_u64(value.number().get()).map_err(codec)?;
    writer.write_fixed(value.digest().as_bytes()).map_err(codec)
}

pub(super) fn harness_revision(
    reader: &mut CanonicalReader<'_>,
) -> Result<HarnessRevisionIdentity, EvolutionError> {
    Ok(HarnessRevisionIdentity::new(
        HarnessId::new(reader.read_fixed().map_err(codec)?).map_err(domain)?,
        RevisionNumber::new(reader.read_u64().map_err(codec)?).map_err(domain)?,
        RevisionDigest::new(digest(reader)?),
    ))
}

pub(super) fn component_id(
    reader: &mut CanonicalReader<'_>,
) -> Result<ComponentId, EvolutionError> {
    ComponentId::new(reader.read_str().map_err(codec)?.to_owned()).map_err(domain)
}

pub(super) fn component_kind(
    reader: &mut CanonicalReader<'_>,
) -> Result<ComponentKind, EvolutionError> {
    let tag = reader.read_u8().map_err(codec)?;
    ComponentKind::ALL.into_iter().find(|kind| kind.tag() == tag).ok_or_else(protocol)
}

pub(super) fn command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    let offset = reader.offset();
    CommandId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    let offset = reader.offset();
    EventId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn campaign_id(
    reader: &mut CanonicalReader<'_>,
) -> Result<EvolutionCampaignId, CodecError> {
    let offset = reader.offset();
    EvolutionCampaignId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) fn project_id(reader: &mut CanonicalReader<'_>) -> Result<ProjectId, CodecError> {
    let offset = reader.offset();
    ProjectId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(super) const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

pub(super) fn semantic(_: impl core::fmt::Display) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, 0)
}

pub(super) const fn codec(_: CodecError) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::Quarantine,
        "F0 semantic payload violates canonical codec bounds",
    )
}

pub(super) fn domain<T>(_: T) -> EvolutionError {
    protocol()
}

pub(super) const fn protocol() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::Quarantine,
        "F0 semantic payload contains an invalid domain value",
    )
}
