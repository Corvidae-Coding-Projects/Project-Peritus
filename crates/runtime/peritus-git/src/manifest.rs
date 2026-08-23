//! Canonical, versioned persistence records for restart-safe Git handles.

mod candidate;
mod snapshot;
mod worktree;

use std::path::Path;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::Sha256Digest;

use crate::{Baseline, CommitId, RegisteredWorktree, SnapshotRef, TreeId};
use crate::{ErrorKind, GitError, ObjectFormat, ObjectId, Operation, RecoveryClass};
use peritus_types::{SnapshotId, WorkspaceId};

pub use candidate::CandidateTreeManifest;
pub use snapshot::CandidateSnapshotManifest;
pub use worktree::WorktreeRegistrationManifest;

const SCHEMA_VERSION: u16 = 1;

const fn writer() -> CanonicalWriter {
    CanonicalWriter::new(CodecLimits::PRODUCTION)
}

fn reader(bytes: &[u8]) -> Result<CanonicalReader<'_>, GitError> {
    if bytes.len() > CodecLimits::PRODUCTION.max_payload_bytes {
        return Err(invalid_manifest());
    }
    Ok(CanonicalReader::new(bytes, CodecLimits::PRODUCTION))
}

fn path_text(path: &Path, operation: Operation) -> Result<&str, GitError> {
    path.to_str().ok_or_else(|| {
        GitError::new(
            ErrorKind::UnsupportedRepository,
            operation,
            RecoveryClass::CorrectRequest,
            "canonical manifest paths must be UTF-8",
        )
    })
}

fn write_object(
    writer: &mut CanonicalWriter,
    object: ObjectId,
) -> Result<(), peritus_codec::CodecError> {
    writer.write_str(&object.to_hex())
}

fn read_object(
    reader: &mut CanonicalReader<'_>,
    format: ObjectFormat,
) -> Result<ObjectId, GitError> {
    let value = reader.read_str().map_err(|_| invalid_manifest())?;
    ObjectId::parse(format, value, Operation::DecodeManifest).map_err(|_| invalid_manifest())
}

const fn format_tag(format: ObjectFormat) -> u8 {
    match format {
        ObjectFormat::Sha1 => 1,
        ObjectFormat::Sha256 => 2,
    }
}

const fn format_from_tag(tag: u8) -> Option<ObjectFormat> {
    match tag {
        1 => Some(ObjectFormat::Sha1),
        2 => Some(ObjectFormat::Sha256),
        _ => None,
    }
}

fn finish(reader: CanonicalReader<'_>) -> Result<(), GitError> {
    reader.finish().map_err(|_| invalid_manifest())
}

fn encoded(
    result: Result<(), peritus_codec::CodecError>,
    writer: CanonicalWriter,
) -> Result<Vec<u8>, GitError> {
    result.map_err(|_| invalid_manifest())?;
    Ok(writer.into_bytes())
}

#[allow(clippy::too_many_arguments)] // Canonical record binds these independent observations.
pub fn candidate_manifest(
    repository: Sha256Digest,
    root: &Path,
    baseline: Baseline,
    head: CommitId,
    tree: TreeId,
    prior: Sha256Digest,
    current: Sha256Digest,
) -> Result<CandidateTreeManifest, GitError> {
    CandidateTreeManifest::new(repository, root, baseline, head, tree, prior, current)
}

#[allow(clippy::too_many_arguments)] // Canonical record binds these independent persisted facts.
pub fn snapshot_manifest(
    repository: Sha256Digest,
    workspace_id: WorkspaceId,
    snapshot_id: SnapshotId,
    parent: CommitId,
    commit: CommitId,
    tree: TreeId,
    reference: SnapshotRef,
    candidate_digest: Sha256Digest,
) -> Result<CandidateSnapshotManifest, GitError> {
    CandidateSnapshotManifest::new(
        repository,
        workspace_id,
        snapshot_id,
        parent,
        commit,
        tree,
        reference,
        candidate_digest,
    )
}

pub fn worktree_manifest(
    worktree: &RegisteredWorktree,
) -> Result<WorktreeRegistrationManifest, GitError> {
    WorktreeRegistrationManifest::new(worktree)
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    peritus_codec::sha256(bytes)
}

fn invalid_manifest() -> GitError {
    GitError::new(
        ErrorKind::GitProtocol,
        Operation::DecodeManifest,
        RecoveryClass::Quarantine,
        "persisted Git manifest is malformed, oversized, or uses an unsupported schema",
    )
}
