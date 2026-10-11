//! Canonical checked identity, source, and environment fields.

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_role::HarnessRole;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use crate::ContextNodeId;
use super::{WorkingCodecError, count};
use super::super::{ObservationId, ObservationKind, ObservationSource, WorkingBinding,
    WorkingEnvironment, WorkingFileDigest, WorkingLimits, WorkingValidity};

pub(super) fn write_binding(w: &mut CanonicalWriter, b: WorkingBinding) -> Result<(), WorkingCodecError> {
    w.write_fixed(b.run().as_bytes())?;
    w.write_fixed(b.workspace().as_bytes())?;
    w.write_fixed(b.task().as_bytes())?;
    w.write_u8(match b.role() { HarnessRole::Writer => 0, HarnessRole::Reviewer => 1, HarnessRole::Fixer => 2, HarnessRole::Evaluator => 3, HarnessRole::Evolver => 4 })?;
    w.write_u64(b.conversation_revision())?;
    Ok(())
}
pub(super) fn read_binding(r: &mut CanonicalReader<'_>) -> Result<WorkingBinding, WorkingCodecError> {
    let run = RunId::new(r.read_fixed()?).map_err(|_| WorkingCodecError::InvalidValue)?;
    let workspace = WorkspaceId::new(r.read_fixed()?).map_err(|_| WorkingCodecError::InvalidValue)?;
    let task = read_id(r)?;
    let role = match r.read_u8()? { 0 => HarnessRole::Writer, 1 => HarnessRole::Reviewer, 2 => HarnessRole::Fixer, 3 => HarnessRole::Evaluator, 4 => HarnessRole::Evolver, _ => return Err(WorkingCodecError::InvalidValue) };
    Ok(WorkingBinding::new(run, workspace, task, role, r.read_u64()?))
}
pub(super) fn read_id(r: &mut CanonicalReader<'_>) -> Result<ContextNodeId, WorkingCodecError> {
    ContextNodeId::new(r.read_fixed()?).map_err(|_| WorkingCodecError::InvalidValue)
}
pub(super) fn write_source(w: &mut CanonicalWriter, source: ObservationSource) -> Result<(), WorkingCodecError> {
    w.write_u64(source.id().get())?;
    w.write_fixed(source.artifact().as_bytes())?;
    w.write_u64(source.artifact_bytes())?;
    w.write_u64(source.start())?;
    w.write_u64(source.end())?;
    w.write_u8(match source.kind() { ObservationKind::HostPolicy => 0, ObservationKind::UserInstruction => 1, ObservationKind::ToolOutput => 2, ObservationKind::AgentMessage => 3 })?;
    Ok(())
}
pub(super) fn read_source(r: &mut CanonicalReader<'_>) -> Result<ObservationSource, WorkingCodecError> {
    let id = ObservationId::new(r.read_u64()?)?;
    let digest = Sha256Digest::new(r.read_fixed()?);
    let bytes = r.read_u64()?;
    let start = r.read_u64()?;
    let end = r.read_u64()?;
    let kind = match r.read_u8()? { 0 => ObservationKind::HostPolicy, 1 => ObservationKind::UserInstruction, 2 => ObservationKind::ToolOutput, 3 => ObservationKind::AgentMessage, _ => return Err(WorkingCodecError::InvalidValue) };
    Ok(ObservationSource::new(id, digest, bytes, start, end, kind)?)
}
pub(super) fn write_files(w: &mut CanonicalWriter, files: &[WorkingFileDigest]) -> Result<(), WorkingCodecError> {
    w.write_collection_len(files.len())?;
    for file in files { w.write_fixed(file.key().as_bytes())?; w.write_fixed(file.digest().as_bytes())?; }
    Ok(())
}
pub(super) fn read_files(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<Vec<WorkingFileDigest>, WorkingCodecError> {
    let length = count(r, maximum)?;
    let mut files = Vec::with_capacity(length);
    for _ in 0..length { files.push(WorkingFileDigest::new(read_id(r)?, Sha256Digest::new(r.read_fixed()?))); }
    Ok(files)
}
pub(super) fn write_environment(w: &mut CanonicalWriter, env: &WorkingEnvironment) -> Result<(), WorkingCodecError> {
    write_binding(w, env.binding())?;
    w.write_fixed(env.candidate().as_bytes())?;
    write_files(w, env.files())
}
pub(super) fn read_environment(r: &mut CanonicalReader<'_>, limits: WorkingLimits) -> Result<WorkingEnvironment, WorkingCodecError> {
    let binding = read_binding(r)?;
    let candidate = Sha256Digest::new(r.read_fixed()?);
    Ok(WorkingEnvironment::new(binding, candidate, read_files(r, limits.entries())?, limits)?)
}
pub(super) fn write_validity(w: &mut CanonicalWriter, valid: &WorkingValidity) -> Result<(), WorkingCodecError> {
    w.write_bool(valid.requires_recheck())?;
    w.write_option_tag(valid.conversation_revision().is_some())?;
    if let Some(revision) = valid.conversation_revision() { w.write_u64(revision)?; }
    w.write_option_tag(valid.candidate().is_some())?;
    if let Some(candidate) = valid.candidate() { w.write_fixed(candidate.as_bytes())?; }
    write_files(w, valid.files())
}
pub(super) fn read_validity(r: &mut CanonicalReader<'_>, limits: WorkingLimits) -> Result<WorkingValidity, WorkingCodecError> {
    let uncertain = r.read_bool()?;
    let revision = if r.read_option_tag()? { Some(r.read_u64()?) } else { None };
    let candidate = if r.read_option_tag()? { Some(Sha256Digest::new(r.read_fixed()?)) } else { None };
    let files = read_files(r, limits.links())?;
    if uncertain {
        if revision.is_some() || candidate.is_some() || !files.is_empty() { return Err(WorkingCodecError::InvalidValue); }
        Ok(WorkingValidity::uncertain())
    } else { Ok(WorkingValidity::new(revision, candidate, files, limits)?) }
}
