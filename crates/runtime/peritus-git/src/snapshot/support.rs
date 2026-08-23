//! Canonical snapshot identities, manifests, reference CAS, and filesystem scans.

use std::ffi::OsString;
use std::path::Path;

use peritus_types::{SnapshotId, WorkspaceId};

use crate::command::{CommandAccess, one_line};
use crate::{
    CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass,
    RegisteredWorktree,
};

use super::{CandidateSnapshot, CandidateTree, SnapshotRef};

const MAX_SCAN_ENTRIES: usize = 200_000;

pub(super) fn validate_candidate_binding(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    candidate: &CandidateTree,
) -> Result<(), GitError> {
    repository.validate_registration(worktree, Operation::CreateSnapshot)?;
    if candidate.repository_digest != repository.identity.digest()
        || candidate.worktree_root != worktree.root()
        || candidate.baseline != worktree.baseline()
    {
        return Err(object_mismatch(
            Operation::CreateSnapshot,
            "candidate belongs to another repository or worktree lineage",
        ));
    }
    Ok(())
}

pub(super) fn retain_ref(
    repository: &GitRepository,
    reference: &SnapshotRef,
    commit: CommitId,
) -> Result<(), GitError> {
    match observe_ref(repository, reference, Operation::CreateSnapshot)? {
        Some(existing) if existing == commit => return Ok(()),
        Some(_) => {
            return Err(GitError::new(
                ErrorKind::SnapshotConflict,
                Operation::CreateSnapshot,
                RecoveryClass::CorrectRequest,
                "snapshot reference already denotes another commit",
            ));
        }
        None => {}
    }
    let arguments = vec![
        OsString::from("update-ref"),
        OsString::from("--create-reflog"),
        OsString::from(reference.as_str()),
        OsString::from(commit.to_string()),
        OsString::from(ObjectId::zero_hex(repository.identity.object_format())),
    ];
    let output = repository.runner.observe(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Write,
        Operation::CreateSnapshot,
        &arguments,
        None,
    )?;
    if output.status.success()
        || observe_ref(repository, reference, Operation::CreateSnapshot)? == Some(commit)
    {
        Ok(())
    } else {
        Err(GitError::new(
            ErrorKind::SnapshotConflict,
            Operation::CreateSnapshot,
            RecoveryClass::Reobserve,
            "snapshot reference raced with another value",
        ))
    }
}

pub(super) fn verify_retained(
    repository: &GitRepository,
    snapshot: &CandidateSnapshot,
    operation: Operation,
) -> Result<(), GitError> {
    match observe_ref(repository, snapshot.reference(), operation)? {
        Some(commit) if commit == snapshot.commit() => Ok(()),
        _ => Err(GitError::new(
            ErrorKind::SnapshotConflict,
            operation,
            RecoveryClass::Reconcile,
            "snapshot reference is missing or denotes another commit",
        )),
    }
}

fn observe_ref(
    repository: &GitRepository,
    reference: &SnapshotRef,
    operation: Operation,
) -> Result<Option<CommitId>, GitError> {
    let quiet_arguments = vec![
        OsString::from("show-ref"),
        OsString::from("--verify"),
        OsString::from("--quiet"),
        OsString::from(reference.as_str()),
    ];
    let quiet = repository.runner.observe(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Read,
        operation,
        &quiet_arguments,
        None,
    )?;
    match quiet.status.code() {
        Some(1) => return Ok(None),
        Some(0) => {}
        _ => return Err(GitError::command(operation, quiet.status.code(), &quiet.stderr)),
    }
    let value_arguments = vec![
        OsString::from("show-ref"),
        OsString::from("--verify"),
        OsString::from("--hash"),
        OsString::from(reference.as_str()),
    ];
    let value = repository.runner.checked(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Read,
        operation,
        &value_arguments,
        None,
    )?;
    Ok(Some(CommitId::checked(ObjectId::parse(
        repository.identity.object_format(),
        one_line(&value.stdout, operation)?,
        operation,
    )?)))
}

pub(super) fn snapshot_ref(workspace_id: WorkspaceId, snapshot_id: SnapshotId) -> SnapshotRef {
    SnapshotRef(format!(
        "refs/peritus/workspaces/{}/snapshots/{}",
        identifier_hex(workspace_id.as_bytes()),
        identifier_hex(snapshot_id.as_bytes())
    ))
}

pub(super) fn identifier_hex(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(32);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

pub(super) fn reject_nested_git_metadata(
    root: &Path,
    operation: Operation,
) -> Result<(), GitError> {
    let mut directories = vec![root.to_owned()];
    let mut seen = 0_usize;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|source| {
            GitError::io(operation, RecoveryClass::Reconcile, "scan worktree entries", source)
        })? {
            let entry = entry.map_err(|source| {
                GitError::io(operation, RecoveryClass::Reconcile, "read worktree entry", source)
            })?;
            seen = seen.checked_add(1).ok_or_else(|| scan_error(operation))?;
            if seen > MAX_SCAN_ENTRIES {
                return Err(scan_error(operation));
            }
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|source| {
                GitError::io(operation, RecoveryClass::Reconcile, "inspect worktree entry", source)
            })?;
            let is_root_git = directory == root && entry.file_name() == ".git";
            if entry.file_name() == ".git" && !is_root_git {
                return Err(GitError::new(
                    ErrorKind::WorktreeConflict,
                    operation,
                    RecoveryClass::CorrectRequest,
                    "nested Git repository or worktree metadata is not supported",
                ));
            }
            if metadata.is_dir() && !is_root_git {
                directories.push(path);
            }
        }
    }
    Ok(())
}

fn scan_error(operation: Operation) -> GitError {
    GitError::new(
        ErrorKind::InvalidInput,
        operation,
        RecoveryClass::CorrectRequest,
        "worktree entry count exceeds the candidate scan limit",
    )
}

pub(super) fn object_mismatch(operation: Operation, detail: &'static str) -> GitError {
    GitError::new(ErrorKind::ObjectMismatch, operation, RecoveryClass::Reobserve, detail)
}
