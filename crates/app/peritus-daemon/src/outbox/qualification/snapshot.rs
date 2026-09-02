//! Real Git snapshot recovery on both sides of retained-reference publication.

mod corruption;
mod disk;
mod git_command;

pub(super) use disk::{recover_snapshot_quota_exhaustion, stage_snapshot_quota_exhaustion};

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use peritus_git::{
    CandidateRequest, CandidateSnapshotManifest, CandidateTree, CreateWorktree, GitError,
    GitRepository, RegisteredWorktree, RepositoryOptions, SnapshotRequest, WorktreeAccess,
    WorktreeName,
};
use peritus_types::{SnapshotId, WorkspaceId};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::verify_empty_journal;
use git_command::{count_snapshot_refs, reference_value, run_git};

pub use corruption::{recover_snapshot_corruption, stage_snapshot_corruption};

const ROOT_NAME: &str = "snapshot-crash-qualification-v1";
const INTENT_FILE: &str = "snapshot-intent-v1";
const MANIFEST_FILE: &str = "snapshot-manifest-v1.bin";

/// Checkpoint retaining a real candidate tree in memory before snapshot publication.
pub struct SnapshotBeforeCheckpoint {
    tree: String,
    reference: String,
    _repository: GitRepository,
    _worktree: RegisteredWorktree,
    _candidate: CandidateTree,
}

impl SnapshotBeforeCheckpoint {
    pub(crate) fn tree(&self) -> &str {
        &self.tree
    }
    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }
}

/// Durable checkpoint after the real synthetic commit and retained ref are published.
pub struct SnapshotAfterCheckpoint {
    commit: String,
    tree: String,
    reference: String,
    manifest_sha256: String,
}

impl SnapshotAfterCheckpoint {
    pub(crate) fn commit(&self) -> &str {
        &self.commit
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
}

/// Direct repository and journal facts observed by a fresh recovery process.
pub struct SnapshotQualification {
    commit: Option<String>,
    tree: String,
    reference: String,
    manifest_sha256: Option<String>,
    journal_verified: bool,
    retained: bool,
    snapshot_refs: u64,
}

impl SnapshotQualification {
    pub(crate) fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }

    pub(crate) fn tree(&self) -> &str {
        &self.tree
    }
    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }

    pub(crate) fn manifest_sha256(&self) -> Option<&str> {
        self.manifest_sha256.as_deref()
    }

    pub(crate) const fn journal_verified(&self) -> bool {
        self.journal_verified
    }
    pub(crate) const fn retained(&self) -> bool {
        self.retained
    }

    pub(crate) const fn snapshot_refs(&self) -> u64 {
        self.snapshot_refs
    }
}

/// Creates a real candidate tree and stops before the snapshot commit operation is invoked.
pub fn stage_snapshot_before_crash(
    config: &DaemonConfig,
) -> Result<SnapshotBeforeCheckpoint, DaemonError> {
    let prepared = prepare_repository(config)?;
    verify_empty_journal(config)?;
    let tree = prepared.candidate.tree().to_string();
    let reference = snapshot_reference();
    write_intent(&prepared.root, &tree, &reference)?;
    if reference_value(&prepared.source, &reference)?.is_some() {
        return Err(snapshot_error("snapshot reference exists before durable publication"));
    }
    Ok(SnapshotBeforeCheckpoint {
        tree,
        reference,
        _repository: prepared.repository,
        _worktree: prepared.worktree,
        _candidate: prepared.candidate,
    })
}

/// Publishes the real deterministic snapshot commit and its compare-and-swap retained ref.
pub fn stage_snapshot_after_crash(
    config: &DaemonConfig,
) -> Result<SnapshotAfterCheckpoint, DaemonError> {
    let prepared = prepare_repository(config)?;
    verify_empty_journal(config)?;
    let snapshot = prepared
        .repository
        .create_snapshot(SnapshotRequest::new(
            &prepared.worktree,
            &prepared.candidate,
            workspace_id()?,
            snapshot_id()?,
            prepared.baseline_commit,
        ))
        .map_err(git_error)?;
    let tree = snapshot.tree().to_string();
    let reference = snapshot.reference().as_str().to_owned();
    write_intent(&prepared.root, &tree, &reference)?;
    let manifest_path = prepared.root.join(MANIFEST_FILE);
    write_new(&manifest_path, snapshot.manifest().bytes())?;
    let manifest_sha256 = digest_hex(snapshot.manifest().bytes());
    let commit = snapshot.commit().to_string();
    if reference_value(&prepared.source, &reference)?.as_deref() != Some(&commit) {
        return Err(snapshot_error("published snapshot reference differs from its exact commit"));
    }
    Ok(SnapshotAfterCheckpoint { commit, tree, reference, manifest_sha256 })
}

/// Reopens the fresh repository and proves that no snapshot was retained before publication.
pub fn recover_snapshot_before_crash(
    config: &DaemonConfig,
) -> Result<SnapshotQualification, DaemonError> {
    let root = qualification_root(config);
    let source = root.join("repository");
    let repository = GitRepository::open(RepositoryOptions::new(&source)).map_err(git_error)?;
    let (tree, reference) = read_intent(&root)?;
    let baseline = repository.resolve_baseline("HEAD").map_err(git_error)?;
    if tree == baseline.tree().to_string() || reference_value(&source, &reference)?.is_some() {
        return Err(snapshot_error("uncommitted snapshot state was falsely retained"));
    }
    let snapshot_refs = count_snapshot_refs(&source)?;
    if snapshot_refs != 0 || root.join(MANIFEST_FILE).exists() {
        return Err(snapshot_error("pre-commit recovery found snapshot publication metadata"));
    }
    Ok(SnapshotQualification {
        commit: None,
        tree,
        reference,
        manifest_sha256: None,
        journal_verified: verify_empty_journal(config)?,
        retained: false,
        snapshot_refs,
    })
}

/// Reopens and fully revalidates the exact committed snapshot manifest and retained ref.
pub fn recover_snapshot_after_crash(
    config: &DaemonConfig,
) -> Result<SnapshotQualification, DaemonError> {
    let root = qualification_root(config);
    let source = root.join("repository");
    let repository = GitRepository::open(RepositoryOptions::new(&source)).map_err(git_error)?;
    let (expected_tree, expected_reference) = read_intent(&root)?;
    let manifest_bytes = fs::read(root.join(MANIFEST_FILE)).map_err(filesystem_error)?;
    let manifest = CandidateSnapshotManifest::decode(&manifest_bytes).map_err(git_error)?;
    let snapshot = repository.reopen_snapshot(&manifest).map_err(git_error)?;
    let commit = snapshot.commit().to_string();
    let tree = snapshot.tree().to_string();
    let reference = snapshot.reference().as_str().to_owned();
    let snapshot_refs = count_snapshot_refs(&source)?;
    if tree != expected_tree
        || reference != expected_reference
        || reference_value(&source, &reference)?.as_deref() != Some(&commit)
        || snapshot_refs != 1
    {
        return Err(snapshot_error("reopened snapshot identity differs from durable state"));
    }
    Ok(SnapshotQualification {
        commit: Some(commit),
        tree,
        reference,
        manifest_sha256: Some(digest_hex(&manifest_bytes)),
        journal_verified: verify_empty_journal(config)?,
        retained: true,
        snapshot_refs,
    })
}

struct PreparedRepository {
    root: PathBuf,
    source: PathBuf,
    repository: GitRepository,
    worktree: RegisteredWorktree,
    candidate: CandidateTree,
    baseline_commit: peritus_git::CommitId,
}

fn prepare_repository(config: &DaemonConfig) -> Result<PreparedRepository, DaemonError> {
    fs::create_dir_all(config.paths().state_root()).map_err(filesystem_error)?;
    let root = qualification_root(config);
    create_private_directory(&root)?;
    let source = root.join("repository");
    run_git(&root, [OsStr::new("init"), OsStr::new("--initial-branch=main"), source.as_os_str()])?;
    write_new(&source.join("README.md"), b"Peritus H1 snapshot baseline\n")?;
    run_git(&source, [OsStr::new("add"), OsStr::new("--all"), OsStr::new("--"), OsStr::new(".")])?;
    run_git(
        &source,
        [
            OsStr::new("commit"),
            OsStr::new("--no-gpg-sign"),
            OsStr::new("-m"),
            OsStr::new("Peritus H1 snapshot baseline"),
        ],
    )?;
    let repository = GitRepository::open(RepositoryOptions::new(&source)).map_err(git_error)?;
    let baseline = repository.resolve_baseline("HEAD").map_err(git_error)?;
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("h1_snapshot").map_err(git_error)?,
            root.join("h1_snapshot"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .map_err(git_error)?;
    write_new(&worktree.root().join("candidate.txt"), b"Peritus H1 retained snapshot candidate\n")?;
    let candidate = repository
        .create_candidate(CandidateRequest::new(&worktree, baseline.commit()))
        .map_err(git_error)?;
    if candidate.tree() == baseline.tree() {
        return Err(snapshot_error("snapshot candidate tree did not change from its baseline"));
    }
    Ok(PreparedRepository {
        root,
        source,
        repository,
        worktree,
        candidate,
        baseline_commit: baseline.commit(),
    })
}

fn write_intent(root: &Path, tree: &str, reference: &str) -> Result<(), DaemonError> {
    write_new(&root.join(INTENT_FILE), format!("{tree}\n{reference}\n").as_bytes())
}

fn read_intent(root: &Path) -> Result<(String, String), DaemonError> {
    let bytes = fs::read(root.join(INTENT_FILE)).map_err(filesystem_error)?;
    if bytes.len() > 1_024 {
        return Err(snapshot_error("snapshot intent exceeded its fixed bound"));
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| snapshot_error("snapshot intent is not UTF-8"))?;
    let fields = text.lines().collect::<Vec<_>>();
    if fields.len() != 2 || fields.iter().any(|field| field.is_empty()) {
        return Err(snapshot_error("snapshot intent is malformed"));
    }
    Ok((fields[0].to_owned(), fields[1].to_owned()))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), DaemonError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(filesystem_error)?;
    file.write_all(bytes).map_err(filesystem_error)?;
    file.sync_all().map_err(filesystem_error)
}

fn create_private_directory(path: &Path) -> Result<(), DaemonError> {
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = fs::DirBuilder::new();
    builder.create(path).map_err(filesystem_error)
}

fn qualification_root(config: &DaemonConfig) -> PathBuf {
    config.paths().state_root().join(ROOT_NAME)
}

fn workspace_id() -> Result<WorkspaceId, DaemonError> {
    WorkspaceId::new([0x3d; 16]).map_err(|_| snapshot_error("fixed workspace ID is invalid"))
}

fn snapshot_id() -> Result<SnapshotId, DaemonError> {
    SnapshotId::new([0x4e; 16]).map_err(|_| snapshot_error("fixed snapshot ID is invalid"))
}

fn snapshot_reference() -> String {
    format!("refs/peritus/workspaces/{}/snapshots/{}", hex(&[0x3d; 16]), hex(&[0x4e; 16]))
}

fn digest_hex(bytes: &[u8]) -> String {
    hex(peritus_codec::sha256(bytes).as_bytes())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

fn git_error(error: GitError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify Git snapshot commit recovery",
        error.to_string(),
        error,
    )
}

fn filesystem_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify Git snapshot commit recovery",
        error.to_string(),
        error,
    )
}

fn snapshot_error(detail: impl Into<String>) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify Git snapshot commit recovery",
        detail,
    )
}
