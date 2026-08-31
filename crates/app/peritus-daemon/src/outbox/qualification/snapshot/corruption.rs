//! Controlled retained-reference divergence and production quarantine qualification.

use std::ffi::OsStr;
use std::fs;

use peritus_git::{
    CandidateSnapshotManifest, ErrorKind, GitRepository, Operation, RepositoryOptions,
};

use crate::{DaemonConfig, DaemonError};

use super::git_command::{reference_value, run_git};
use super::{
    MANIFEST_FILE, digest_hex, git_error, prepare_repository, qualification_root, snapshot_error,
    verify_empty_journal, write_intent, write_new,
};

/// Exact identities retained after a healthy snapshot reference is made divergent.
pub struct SnapshotCorruptionCheckpoint {
    expected_commit: String,
    divergent_commit: String,
    reference: String,
    manifest_sha256: String,
}

impl SnapshotCorruptionCheckpoint {
    pub(crate) fn expected_commit(&self) -> &str {
        &self.expected_commit
    }
    pub(crate) fn divergent_commit(&self) -> &str {
        &self.divergent_commit
    }
    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }
    pub(crate) fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
}

/// Fresh-process facts after the divergent reference was removed from active use.
pub struct SnapshotCorruptionObservation {
    reference: String,
    quarantine_reference: String,
    quarantined_commit: String,
}

impl SnapshotCorruptionObservation {
    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }
    pub(crate) fn quarantine_reference(&self) -> &str {
        &self.quarantine_reference
    }
    pub(crate) fn quarantined_commit(&self) -> &str {
        &self.quarantined_commit
    }
    pub(crate) const fn journal_verified(&self) -> bool {
        true
    }
    pub(crate) const fn corruption_detected(&self) -> bool {
        true
    }
    pub(crate) const fn mutation_admitted(&self) -> bool {
        false
    }
}

/// Publishes a real snapshot, then redirects its retained ref to the baseline commit.
pub fn stage_snapshot_corruption(
    config: &DaemonConfig,
) -> Result<SnapshotCorruptionCheckpoint, DaemonError> {
    let prepared = prepare_repository(config)?;
    verify_empty_journal(config)?;
    let snapshot = prepared
        .repository
        .create_snapshot(peritus_git::SnapshotRequest::new(
            &prepared.worktree,
            &prepared.candidate,
            super::workspace_id()?,
            super::snapshot_id()?,
            prepared.baseline_commit,
        ))
        .map_err(git_error)?;
    let reference = snapshot.reference().as_str().to_owned();
    let expected_commit = snapshot.commit().to_string();
    let divergent_commit = prepared.baseline_commit.to_string();
    write_intent(&prepared.root, &snapshot.tree().to_string(), &reference)?;
    write_new(&prepared.root.join(MANIFEST_FILE), snapshot.manifest().bytes())?;
    run_git(
        &prepared.source,
        [
            OsStr::new("update-ref"),
            OsStr::new(&reference),
            OsStr::new(&divergent_commit),
            OsStr::new(&expected_commit),
        ],
    )?;
    if reference_value(&prepared.source, &reference)?.as_deref() != Some(&divergent_commit) {
        return Err(snapshot_error("snapshot divergence injection changed the wrong reference"));
    }
    let failure = prepared
        .repository
        .reopen_snapshot(snapshot.manifest())
        .expect_err("divergent qualification snapshot must not reopen");
    if failure.kind() != ErrorKind::SnapshotConflict
        || failure.operation() != Operation::ReopenSnapshot
    {
        return Err(snapshot_error("snapshot divergence produced the wrong failure category"));
    }
    Ok(SnapshotCorruptionCheckpoint {
        expected_commit,
        divergent_commit,
        reference,
        manifest_sha256: digest_hex(snapshot.manifest().bytes()),
    })
}

/// Reopens the repository and atomically quarantines the divergent retained reference.
pub fn recover_snapshot_corruption(
    config: &DaemonConfig,
) -> Result<SnapshotCorruptionObservation, DaemonError> {
    let root = qualification_root(config);
    let source = root.join("repository");
    let repository = GitRepository::open(RepositoryOptions::new(&source)).map_err(git_error)?;
    let manifest_bytes = fs::read(root.join(MANIFEST_FILE)).map_err(super::filesystem_error)?;
    let manifest = CandidateSnapshotManifest::decode(&manifest_bytes).map_err(git_error)?;
    let failure = repository
        .reopen_snapshot(&manifest)
        .expect_err("divergent qualification snapshot must not reopen");
    if failure.kind() != ErrorKind::SnapshotConflict {
        return Err(snapshot_error("fresh process reported the wrong snapshot divergence"));
    }
    let contained = repository.quarantine_snapshot(&manifest).map_err(git_error)?;
    let repeated = repository.quarantine_snapshot(&manifest).map_err(git_error)?;
    if contained != repeated
        || reference_value(&source, contained.active_reference().as_str())?.is_some()
        || reference_value(&source, contained.quarantine_reference().as_str())?.as_deref()
            != Some(&contained.observed_commit().to_string())
        || !verify_empty_journal(config)?
    {
        return Err(snapshot_error("snapshot divergence was not durably quarantined"));
    }
    Ok(SnapshotCorruptionObservation {
        reference: contained.active_reference().as_str().to_owned(),
        quarantine_reference: contained.quarantine_reference().as_str().to_owned(),
        quarantined_commit: contained.observed_commit().to_string(),
    })
}
