//! Snapshot publication rollback when its durable manifest exceeds storage quota.

use std::fs;
use std::path::{Path, PathBuf};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, ArtifactStoreError, EncryptionMetadata, MediaType,
    ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_git::{GitRepository, RepositoryOptions, SnapshotRequest};
use peritus_types::{ActionId, EventId, Generation, RevisionNumber, SnapshotId};
use peritus_workspace::{WorkspaceManifest, finalize_snapshot_manifest};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::git_command::count_snapshot_refs;
use super::{
    PreparedRepository, git_error, prepare_repository, qualification_root, snapshot_error,
    workspace_id, write_new,
};
use crate::qualification::verify_empty_journal;

const ARTIFACT_DIRECTORY: &str = "snapshot-manifest-artifacts";
const INTENT_FILE: &str = "snapshot-disk-intent-v1";
const FILLER: [u8; 4_096] = [0x7a; 4_096];
const QUOTA_BYTES: u64 = FILLER.len() as u64;
const SUCCESSOR_ID: [u8; 16] = [0x5f; 16];

/// Direct facts after the real manifest quota rejection and retained-ref compensation.
pub(super) struct SnapshotQuotaCheckpoint {
    filler_sha256: String,
    tree: String,
    reference: String,
    manifest_sha256: String,
    quota_bytes: u64,
    snapshot_refs: u64,
    temporary_files: u64,
    object_files: u64,
}

impl SnapshotQuotaCheckpoint {
    pub(crate) fn filler_sha256(&self) -> &str {
        &self.filler_sha256
    }
    pub(crate) fn tree(&self) -> &str {
        &self.tree
    }
    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }
    pub(crate) fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub(crate) const fn quota_bytes(&self) -> u64 {
        self.quota_bytes
    }
    pub(crate) const fn snapshot_refs(&self) -> u64 {
        self.snapshot_refs
    }
    pub(crate) const fn temporary_files(&self) -> u64 {
        self.temporary_files
    }
    pub(crate) const fn object_files(&self) -> u64 {
        self.object_files
    }
}

/// Fresh-process facts proving no ref, manifest object, or temporary artifact survived.
pub(super) struct SnapshotQuotaQualification {
    checkpoint: SnapshotQuotaCheckpoint,
    used_bytes: u64,
    journal_verified: bool,
}

impl SnapshotQuotaQualification {
    pub(crate) const fn checkpoint(&self) -> &SnapshotQuotaCheckpoint {
        &self.checkpoint
    }
    pub(crate) const fn used_bytes(&self) -> u64 {
        self.used_bytes
    }
    pub(crate) const fn journal_verified(&self) -> bool {
        self.journal_verified
    }
}

pub(in crate::outbox::qualification) fn stage_snapshot_quota_exhaustion(
    config: &DaemonConfig,
) -> Result<String, DaemonError> {
    let checkpoint = stage_snapshot_quota_checkpoint(config)?;
    Ok(format!(
        "peritus-qualification disk-snapshot-commit-stage filler_sha256={} tree={} reference={} manifest_sha256={} quota_bytes={} snapshot_refs={} temporary_files={} object_files={}",
        checkpoint.filler_sha256(),
        checkpoint.tree(),
        checkpoint.reference(),
        checkpoint.manifest_sha256(),
        checkpoint.quota_bytes(),
        checkpoint.snapshot_refs(),
        checkpoint.temporary_files(),
        checkpoint.object_files(),
    ))
}

pub(in crate::outbox::qualification) fn recover_snapshot_quota_exhaustion(
    config: &DaemonConfig,
) -> Result<String, DaemonError> {
    let observation = recover_snapshot_quota_checkpoint(config)?;
    let checkpoint = observation.checkpoint();
    Ok(format!(
        "peritus-qualification disk-snapshot-commit-recover filler_sha256={} tree={} reference={} manifest_sha256={} quota_bytes={} used_bytes={} journal_verified={} snapshot_refs={} temporary_files={} object_files={}",
        checkpoint.filler_sha256(),
        checkpoint.tree(),
        checkpoint.reference(),
        checkpoint.manifest_sha256(),
        checkpoint.quota_bytes(),
        observation.used_bytes(),
        observation.journal_verified(),
        checkpoint.snapshot_refs(),
        checkpoint.temporary_files(),
        checkpoint.object_files(),
    ))
}

/// Creates a real Git snapshot, rejects its production manifest at the artifact quota, and
/// requires the publication transaction to release the retained reference.
fn stage_snapshot_quota_checkpoint(
    config: &DaemonConfig,
) -> Result<SnapshotQuotaCheckpoint, DaemonError> {
    let prepared = prepare_repository(config)?;
    verify_empty_journal(config)?;
    let snapshot = create_snapshot(&prepared)?;
    let reference = snapshot.reference().as_str().to_owned();
    let tree = snapshot.tree().to_string();
    let manifest = workspace_manifest(&prepared, &snapshot)?;
    let manifest_sha256 = manifest.digest().to_hex();
    let store = open_store(&prepared.root)?;
    let filler = fill_quota(&store)?;
    let failure = match finalize_snapshot_manifest(
        &prepared.repository,
        &snapshot,
        &manifest,
        &store,
        event_id()?,
    ) {
        Ok(_) => return Err(snapshot_error("snapshot manifest unexpectedly passed its quota")),
        Err(failure) => failure,
    };
    if failure.compensation_failure().is_some() {
        return Err(snapshot_error("snapshot quota compensation did not release the retained ref"));
    }
    let snapshot_refs = count_snapshot_refs(&prepared.source)?;
    let (temporary_files, object_files) = artifact_counts(&prepared.root)?;
    let used_bytes = store.quota_snapshot(0).map_err(artifact_error)?.used_bytes();
    if snapshot_refs != 0
        || temporary_files != 0
        || object_files != 1
        || used_bytes != QUOTA_BYTES
        || store.verify(filler).map_err(artifact_error)?.size() != QUOTA_BYTES
    {
        return Err(snapshot_error("snapshot quota rejection left published or temporary state"));
    }
    let checkpoint = SnapshotQuotaCheckpoint {
        filler_sha256: filler.to_hex(),
        tree,
        reference,
        manifest_sha256,
        quota_bytes: QUOTA_BYTES,
        snapshot_refs,
        temporary_files,
        object_files,
    };
    write_intent(&prepared.root, &checkpoint)?;
    Ok(checkpoint)
}

/// Reopens the repository and artifact store to verify the exact compensated absence.
fn recover_snapshot_quota_checkpoint(
    config: &DaemonConfig,
) -> Result<SnapshotQuotaQualification, DaemonError> {
    let root = qualification_root(config);
    let source = root.join("repository");
    let _repository = GitRepository::open(RepositoryOptions::new(&source)).map_err(git_error)?;
    let mut checkpoint = read_intent(&root)?;
    checkpoint.snapshot_refs = count_snapshot_refs(&source)?;
    (checkpoint.temporary_files, checkpoint.object_files) = artifact_counts(&root)?;
    let store = open_store(&root)?;
    let used_bytes = store.quota_snapshot(0).map_err(artifact_error)?.used_bytes();
    let journal_verified = verify_empty_journal(config)?;
    let filler = artifact_digest(&checkpoint.filler_sha256)?;
    if checkpoint.snapshot_refs != 0
        || checkpoint.temporary_files != 0
        || checkpoint.object_files != 1
        || used_bytes != QUOTA_BYTES
        || store.verify(filler).map_err(artifact_error)?.size() != QUOTA_BYTES
        || store.read(filler, QUOTA_BYTES).map_err(artifact_error)? != FILLER
        || !store.reference_roots().map_err(artifact_error)?.contains(&filler)
    {
        return Err(snapshot_error(
            "reopened snapshot quota state differs from compensated absence",
        ));
    }
    Ok(SnapshotQuotaQualification { checkpoint, used_bytes, journal_verified })
}

fn create_snapshot(
    prepared: &PreparedRepository,
) -> Result<peritus_git::CandidateSnapshot, DaemonError> {
    prepared
        .repository
        .create_snapshot(SnapshotRequest::new(
            &prepared.worktree,
            &prepared.candidate,
            workspace_id()?,
            successor_id()?,
            prepared.baseline_commit,
        ))
        .map_err(git_error)
}

fn workspace_manifest(
    prepared: &PreparedRepository,
    snapshot: &peritus_git::CandidateSnapshot,
) -> Result<WorkspaceManifest, DaemonError> {
    Ok(WorkspaceManifest::candidate(
        workspace_id()?,
        Generation::first(),
        RevisionNumber::first(),
        RevisionNumber::new(2).map_err(|_| snapshot_error("snapshot revision is invalid"))?,
        ActionId::new([0x60; 16]).map_err(|_| snapshot_error("snapshot action is invalid"))?,
        peritus_codec::sha256(b"peritus/h1/snapshot-quota/action/v1\0"),
        snapshot.tree(),
        prepared.candidate.manifest_digest(),
    ))
}

fn open_store(root: &Path) -> Result<ArtifactStore, DaemonError> {
    let config = StoreConfig::new(root.join(ARTIFACT_DIRECTORY), QUOTA_BYTES, QUOTA_BYTES)
        .map_err(artifact_error)?;
    ArtifactStore::open(config).map_err(artifact_error)
}

fn fill_quota(store: &ArtifactStore) -> Result<ArtifactDigest, DaemonError> {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(&FILLER));
    let request = WriteRequest::new(
        digest,
        QUOTA_BYTES,
        QUOTA_BYTES,
        MediaType::new("application/octet-stream").map_err(artifact_error)?,
        EncryptionMetadata::unencrypted(),
        EventId::new([0x62; 16]).map_err(|_| snapshot_error("filler event is invalid"))?,
    );
    let mut writer = store.begin_owned_write(request).map_err(artifact_error)?;
    writer.write_chunk(&FILLER).map_err(artifact_error)?;
    store.complete_write(writer).map_err(artifact_error)?;
    store
        .add_reference(
            ReferenceOwner::evidence(peritus_codec::sha256(
                b"peritus/h1/snapshot-quota/filler-owner/v1\0",
            )),
            digest,
        )
        .map_err(artifact_error)?;
    Ok(digest)
}

fn artifact_counts(root: &Path) -> Result<(u64, u64), DaemonError> {
    let artifacts = root.join(ARTIFACT_DIRECTORY);
    Ok((
        count_files(&artifacts.join("temporary"))?,
        count_files_recursive(&artifacts.join("objects"))?,
    ))
}

fn count_files(path: &Path) -> Result<u64, DaemonError> {
    count_files_inner(path, false)
}

fn count_files_recursive(path: &Path) -> Result<u64, DaemonError> {
    count_files_inner(path, true)
}

fn count_files_inner(path: &Path, recursive: bool) -> Result<u64, DaemonError> {
    if !path.exists() {
        return Ok(0);
    }
    let mut pending = vec![PathBuf::from(path)];
    let mut count = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(filesystem_error)? {
            let entry = entry.map_err(filesystem_error)?;
            let kind = entry.file_type().map_err(filesystem_error)?;
            if recursive && kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| snapshot_error("snapshot artifact count overflowed"))?;
            }
        }
    }
    Ok(count)
}

fn write_intent(root: &Path, checkpoint: &SnapshotQuotaCheckpoint) -> Result<(), DaemonError> {
    let bytes = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        checkpoint.filler_sha256,
        checkpoint.tree,
        checkpoint.reference,
        checkpoint.manifest_sha256,
        checkpoint.quota_bytes
    );
    write_new(&root.join(INTENT_FILE), bytes.as_bytes())
}

fn read_intent(root: &Path) -> Result<SnapshotQuotaCheckpoint, DaemonError> {
    let bytes = fs::read(root.join(INTENT_FILE)).map_err(filesystem_error)?;
    if bytes.len() > 1_024 {
        return Err(snapshot_error("snapshot quota intent exceeded its fixed bound"));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| snapshot_error("snapshot quota intent is not UTF-8"))?;
    let fields = text.lines().collect::<Vec<_>>();
    if fields.len() != 5 || fields.iter().any(|field| field.is_empty()) {
        return Err(snapshot_error("snapshot quota intent is malformed"));
    }
    let quota_bytes = fields[4]
        .parse::<u64>()
        .map_err(|_| snapshot_error("snapshot quota intent has an invalid quota"))?;
    if quota_bytes != QUOTA_BYTES {
        return Err(snapshot_error("snapshot quota intent changed its fixed quota"));
    }
    Ok(SnapshotQuotaCheckpoint {
        filler_sha256: fields[0].to_owned(),
        tree: fields[1].to_owned(),
        reference: fields[2].to_owned(),
        manifest_sha256: fields[3].to_owned(),
        quota_bytes,
        snapshot_refs: 0,
        temporary_files: 0,
        object_files: 0,
    })
}

fn artifact_digest(value: &str) -> Result<ArtifactDigest, DaemonError> {
    let bytes = decode_sha256(value)?;
    Ok(ArtifactDigest::from_sha256(peritus_types::Sha256Digest::new(bytes)))
}

fn decode_sha256(value: &str) -> Result<[u8; 32], DaemonError> {
    if value.len() != 64 {
        return Err(snapshot_error("snapshot filler digest is not canonical SHA-256"));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_nibble(value: u8) -> Result<u8, DaemonError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(snapshot_error("snapshot filler digest contains non-hexadecimal bytes")),
    }
}

fn successor_id() -> Result<SnapshotId, DaemonError> {
    SnapshotId::new(SUCCESSOR_ID).map_err(|_| snapshot_error("snapshot successor is invalid"))
}

fn event_id() -> Result<EventId, DaemonError> {
    EventId::new([0x61; 16]).map_err(|_| snapshot_error("snapshot event is invalid"))
}

fn artifact_error(error: ArtifactStoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify snapshot manifest quota recovery",
        error.to_string(),
        error,
    )
}

fn filesystem_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "inspect snapshot quota state",
        error.to_string(),
        error,
    )
}
