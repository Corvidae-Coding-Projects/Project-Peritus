//! Canonical encodings shared by protocol message families.

#![allow(
    clippy::missing_errors_doc,
    reason = "private wire helpers return the complete CodecError vocabulary"
)]

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_policy::ActorRole;
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, WorkspaceId,
};

pub fn write_id(writer: &mut CanonicalWriter, bytes: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(bytes)
}

pub fn read_id<T>(
    reader: &mut CanonicalReader<'_>,
    checked: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    checked(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn write_digest(
    writer: &mut CanonicalWriter,
    digest: &peritus_types::Sha256Digest,
) -> Result<(), CodecError> {
    writer.write_fixed(digest.as_bytes())
}

pub fn read_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<peritus_types::Sha256Digest, CodecError> {
    Ok(peritus_types::Sha256Digest::new(reader.read_fixed()?))
}

pub fn write_option_digest(
    writer: &mut CanonicalWriter,
    value: Option<peritus_types::Sha256Digest>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_digest(writer, &value)?;
    }
    Ok(())
}

pub fn read_option_digest(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<peritus_types::Sha256Digest>, CodecError> {
    if reader.read_option_tag()? { read_digest(reader).map(Some) } else { Ok(None) }
}

pub fn write_option_id<T>(
    writer: &mut CanonicalWriter,
    value: Option<T>,
    bytes: impl FnOnce(T) -> [u8; 16],
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_fixed(&bytes(value))?;
    }
    Ok(())
}

pub fn read_option_id<T>(
    reader: &mut CanonicalReader<'_>,
    checked: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
) -> Result<Option<T>, CodecError> {
    if reader.read_option_tag()? { read_id(reader, checked).map(Some) } else { Ok(None) }
}

pub fn write_role(writer: &mut CanonicalWriter, role: ActorRole) -> Result<(), CodecError> {
    let tag = match role {
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
    };
    writer.write_u16(tag)
}

pub fn read_role(reader: &mut CanonicalReader<'_>) -> Result<ActorRole, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
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
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

pub fn write_revision(
    writer: &mut CanonicalWriter,
    revision: &RevisionTuple,
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
    let acceptance_spec_id = read_id(reader, AcceptanceSpecId::new)?;
    let harness_id = read_id(reader, HarnessId::new)?;
    let workspace_id = read_id(reader, WorkspaceId::new)?;
    let generation_offset = reader.offset();
    let workspace_generation = Generation::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, generation_offset))?;
    let revision_offset = reader.offset();
    let workspace_revision = RevisionNumber::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, revision_offset))?;
    let policy_id = read_id(reader, PolicyId::new)?;
    let provider_profile_id = read_id(reader, ProviderProfileId::new)?;
    Ok(RevisionTuple::new(
        acceptance_spec_id,
        harness_id,
        workspace_id,
        workspace_generation,
        workspace_revision,
        policy_id,
        provider_profile_id,
    ))
}
