//! Canonical snapshot identities, manifests, reference CAS, and filesystem scans.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use peritus_types::{SnapshotId, WorkspaceId};

use crate::command::{CommandAccess, one_line};
use crate::repository::strings;
use crate::{
    CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass,
    RegisteredWorktree,
};

use super::{CandidateSnapshot, CandidateTree, SnapshotRef};

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

/// Durably retains canonical snapshot evidence before the active snapshot reference is published.
pub(super) fn retain_manifest(
    repository: &GitRepository,
    workspace_id: WorkspaceId,
    snapshot_id: SnapshotId,
    bytes: &[u8],
) -> Result<(), GitError> {
    let reference = snapshot_manifest_ref(workspace_id, snapshot_id);
    if let Some(existing) = observe_blob_ref(repository, &reference, Operation::CreateSnapshot)? {
        if existing == bytes {
            return Ok(());
        }
        return Err(GitError::new(
            ErrorKind::SnapshotConflict,
            Operation::CreateSnapshot,
            RecoveryClass::CorrectRequest,
            "snapshot manifest reference already contains different bytes",
        ));
    }
    let hash_arguments =
        vec![OsString::from("hash-object"), OsString::from("-w"), OsString::from("--stdin")];
    let output = repository.runner.checked(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Write,
        Operation::CreateSnapshot,
        &hash_arguments,
        Some(bytes),
    )?;
    let object = ObjectId::parse(
        repository.identity.object_format(),
        one_line(&output.stdout, Operation::CreateSnapshot)?,
        Operation::CreateSnapshot,
    )?;
    let arguments = vec![
        OsString::from("update-ref"),
        OsString::from("--create-reflog"),
        OsString::from(reference.as_str()),
        OsString::from(object.to_hex()),
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
        || observe_blob_ref(repository, &reference, Operation::CreateSnapshot)?.as_deref()
            == Some(bytes)
    {
        Ok(())
    } else {
        Err(GitError::new(
            ErrorKind::SnapshotConflict,
            Operation::CreateSnapshot,
            RecoveryClass::Reobserve,
            "snapshot manifest reference raced with another value",
        ))
    }
}

pub(super) fn read_manifest(
    repository: &GitRepository,
    workspace_id: WorkspaceId,
    snapshot_id: SnapshotId,
    operation: Operation,
) -> Result<Option<Vec<u8>>, GitError> {
    observe_blob_ref(repository, &snapshot_manifest_ref(workspace_id, snapshot_id), operation)
}

pub(super) fn release_snapshot_refs(
    repository: &GitRepository,
    snapshot: &CandidateSnapshot,
) -> Result<(), GitError> {
    let active = snapshot.reference();
    let manifest_ref = snapshot_manifest_ref(snapshot.workspace_id(), snapshot.snapshot_id());
    let Some(bytes) = observe_blob_ref(repository, &manifest_ref, Operation::ReleaseSnapshot)?
    else {
        let args = vec![
            OsString::from("update-ref"),
            OsString::from("-d"),
            OsString::from(active.as_str()),
            OsString::from(snapshot.commit().to_string()),
        ];
        repository.checked_repo_command(
            Operation::ReleaseSnapshot,
            CommandAccess::Write,
            &args,
            None,
        )?;
        return Ok(());
    };
    if bytes != snapshot.manifest().bytes() {
        return Err(GitError::new(
            ErrorKind::SnapshotConflict,
            Operation::ReleaseSnapshot,
            RecoveryClass::Reconcile,
            "snapshot manifest reference differs during atomic release",
        ));
    }
    let hash_args = vec![OsString::from("hash-object"), OsString::from("--stdin")];
    let hash = repository.runner.checked(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Read,
        Operation::ReleaseSnapshot,
        &hash_args,
        Some(&bytes),
    )?;
    let object = one_line(&hash.stdout, Operation::ReleaseSnapshot)?;
    let script = format!(
        "delete {} {}\ndelete {} {}\n",
        active.as_str(),
        snapshot.commit(),
        manifest_ref.as_str(),
        object
    );
    repository.checked_repo_command(
        Operation::ReleaseSnapshot,
        CommandAccess::Write,
        &[OsString::from("update-ref"), OsString::from("--stdin")],
        Some(script.as_bytes()),
    )?;
    Ok(())
}

fn snapshot_manifest_ref(workspace_id: WorkspaceId, snapshot_id: SnapshotId) -> SnapshotRef {
    SnapshotRef(format!(
        "refs/peritus/workspaces/{}/snapshot-manifests/{}",
        identifier_hex(workspace_id.as_bytes()),
        identifier_hex(snapshot_id.as_bytes())
    ))
}

fn observe_blob_ref(
    repository: &GitRepository,
    reference: &SnapshotRef,
    operation: Operation,
) -> Result<Option<Vec<u8>>, GitError> {
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
    let hash_args = vec![
        OsString::from("show-ref"),
        OsString::from("--verify"),
        OsString::from("--hash"),
        OsString::from(reference.as_str()),
    ];
    let hash = repository.runner.checked(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Read,
        operation,
        &hash_args,
        None,
    )?;
    let object = ObjectId::parse(
        repository.identity.object_format(),
        one_line(&hash.stdout, operation)?,
        operation,
    )?;
    let args =
        vec![OsString::from("cat-file"), OsString::from("blob"), OsString::from(object.to_hex())];
    let blob = repository.runner.checked(
        repository.control_cwd(),
        Some(repository.common_location()),
        CommandAccess::Read,
        operation,
        &args,
        None,
    )?;
    Ok(Some(blob.stdout))
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

pub(super) fn observe_ref(
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

pub(super) fn quarantine_ref(workspace_id: WorkspaceId, snapshot_id: SnapshotId) -> SnapshotRef {
    SnapshotRef(format!(
        "refs/peritus/quarantine/workspaces/{}/snapshots/{}",
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

pub fn inspect_nested_git_metadata(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    nested: &[super::RegisteredNestedRepository],
    include_ignored: bool,
    target_tree: Option<crate::TreeId>,
    operation: Operation,
) -> Result<(), GitError> {
    super::nested::validate(repository, worktree, nested, operation)?;
    let root = worktree.root();
    let owned = owned_directories(repository, worktree, target_tree, operation)?;
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        super::nested::check_cancelled(repository, operation)?;
        for entry in std::fs::read_dir(&directory).map_err(|source| {
            GitError::io(operation, RecoveryClass::Reconcile, "scan worktree entries", source)
        })? {
            super::nested::check_cancelled(repository, operation)?;
            let entry = entry.map_err(|source| {
                GitError::io(operation, RecoveryClass::Reconcile, "read worktree entry", source)
            })?;
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
                    "nested Git metadata requires an explicit independent repository registration",
                ));
            }
            if metadata.is_dir()
                && !is_root_git
                && !nested.iter().any(|registration| registration.root() == path)
                && (include_ignored
                    || owned.contains(&path)
                    || !is_ignored(repository, worktree, root, &path, operation)?)
            {
                directories.push(path);
            }
        }
    }
    Ok(())
}

/// Records only presently nonignored untracked leaves before restoration can change ignore rules.
pub(super) fn cleanup_inventory(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
) -> Result<Vec<PathBuf>, GitError> {
    let operation = Operation::RestoreSnapshot;
    let output = repository.runner.checked(
        worktree.root(),
        Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
        CommandAccess::Read,
        operation,
        &strings(&["ls-files", "--others", "--exclude-standard", "-z"]),
        None,
    )?;
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|bytes| {
            super::nested::check_cancelled(repository, operation)?;
            inventory_path(bytes, operation)
        })
        .collect()
}

pub(super) fn clean_inventory(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    paths: &[PathBuf],
) -> Result<(), GitError> {
    // A command batch bounds argv transport, never the number of files in the operation.
    for batch in paths.chunks(64) {
        let mut args = strings(&["clean", "-fd", "--"]);
        args.extend(batch.iter().map(|path| path.as_os_str().to_owned()));
        repository.runner.checked(
            worktree.root(),
            Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Write,
            Operation::RestoreSnapshot,
            &args,
            None,
        )?;
    }
    Ok(())
}

fn owned_directories(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    target_tree: Option<crate::TreeId>,
    operation: Operation,
) -> Result<BTreeSet<PathBuf>, GitError> {
    let mut arguments = vec![strings(&["ls-files", "--cached", "-z"])];
    if let Some(tree) = target_tree {
        arguments.push(vec![
            OsString::from("ls-tree"),
            OsString::from("--name-only"),
            OsString::from("-rz"),
            OsString::from(tree.to_string()),
        ]);
    }
    let mut directories = BTreeSet::new();
    for args in arguments {
        let output = repository.runner.checked(
            worktree.root(),
            Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Read,
            operation,
            &args,
            None,
        )?;
        for bytes in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
            super::nested::check_cancelled(repository, operation)?;
            let path = inventory_path(bytes, operation)?;
            // Include the leaf because a gitlink itself names an owned directory boundary.
            for ancestor in path.ancestors() {
                directories.insert(worktree.root().join(ancestor));
            }
        }
    }
    Ok(directories)
}

pub(super) fn inventory_path(bytes: &[u8], operation: Operation) -> Result<PathBuf, GitError> {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt as _;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    };
    #[cfg(not(unix))]
    let path = std::str::from_utf8(bytes).map(PathBuf::from).map_err(|_| {
        GitError::new(
            ErrorKind::GitProtocol,
            operation,
            RecoveryClass::Reconcile,
            "Git inventory path is not UTF-8",
        )
    })?;
    if path.is_absolute()
        || path.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(GitError::new(
            ErrorKind::GitProtocol,
            operation,
            RecoveryClass::Reconcile,
            "Git inventory path escapes the worktree",
        ));
    }
    Ok(path)
}

fn is_ignored(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    root: &Path,
    path: &Path,
    operation: Operation,
) -> Result<bool, GitError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        GitError::new(
            ErrorKind::WorktreeConflict,
            operation,
            RecoveryClass::Reconcile,
            "worktree child escaped its registered root during candidate inspection",
        )
    })?;
    let arguments = vec![
        OsString::from("check-ignore"),
        OsString::from("--quiet"),
        OsString::from("--no-index"),
        OsString::from("--"),
        relative.as_os_str().to_owned(),
    ];
    let output = repository.runner.observe(
        root,
        Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
        CommandAccess::ReadWithoutLiteralPathspecs,
        operation,
        &arguments,
        None,
    )?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(GitError::new(
            ErrorKind::WorktreeConflict,
            operation,
            RecoveryClass::Reconcile,
            "Git could not classify candidate directory ownership",
        )),
    }
}

pub(super) fn object_mismatch(operation: Operation, detail: &'static str) -> GitError {
    GitError::new(ErrorKind::ObjectMismatch, operation, RecoveryClass::Reobserve, detail)
}
