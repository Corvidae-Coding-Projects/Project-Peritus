//! Recovery of a Git-created worktree whose typed handle was not persisted.

use std::path::PathBuf;

use crate::command::{CommandAccess, one_line};
use crate::repository::strings;
use crate::{CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass};

use super::{CreateWorktree, RecoverWorktree, RegisteredWorktree};

pub(super) fn recover_existing(
    repository: &GitRepository,
    request: CreateWorktree,
) -> Result<RegisteredWorktree, GitError> {
    repository.reject_external_filters(Operation::ReopenWorktree, repository.common_location())?;
    let destination =
        validate_existing_destination(repository, &request.name, &request.destination)?;
    let root = std::fs::canonicalize(&destination).map_err(|source| {
        GitError::io(
            Operation::ReopenWorktree,
            RecoveryClass::Reconcile,
            "canonicalize existing worktree",
            source,
        )
    })?;
    if root != destination {
        return Err(conflict("existing worktree destination is not canonical"));
    }
    let git_dir =
        super::lifecycle::discover_worktree_git_dir(repository, &root, Operation::ReopenWorktree)?;
    repository.reject_external_filters(
        Operation::ReopenWorktree,
        GitRepository::worktree_location(&root, &git_dir),
    )?;
    let registration = RegisteredWorktree {
        repository_digest: repository.identity.digest(),
        name: request.name,
        root,
        git_dir,
        baseline: request.baseline,
        access: request.access,
    };
    let observed = repository.inspect_worktree(&registration)?;
    if observed.head() != registration.baseline.commit() || !observed.is_detached() {
        return Err(GitError::new(
            ErrorKind::ObjectMismatch,
            Operation::ReopenWorktree,
            RecoveryClass::Reconcile,
            "existing worktree does not have the exact detached baseline",
        ));
    }
    Ok(registration)
}

pub(super) fn recover_current(
    repository: &GitRepository,
    request: RecoverWorktree,
) -> Result<RegisteredWorktree, GitError> {
    repository.reject_external_filters(Operation::ReopenWorktree, repository.common_location())?;
    let destination =
        validate_existing_destination(repository, &request.name, &request.destination)?;
    let root = std::fs::canonicalize(&destination).map_err(|source| {
        GitError::io(
            Operation::ReopenWorktree,
            RecoveryClass::Reconcile,
            "canonicalize existing worktree",
            source,
        )
    })?;
    if root != destination {
        return Err(conflict("existing worktree destination is not canonical"));
    }
    let git_dir =
        super::lifecycle::discover_worktree_git_dir(repository, &root, Operation::ReopenWorktree)?;
    let worktree_location = GitRepository::worktree_location(&root, &git_dir);
    repository.reject_external_filters(Operation::ReopenWorktree, worktree_location)?;
    let location = Some(worktree_location);
    let output = repository.runner.checked(
        &root,
        location,
        CommandAccess::Read,
        Operation::ReopenWorktree,
        &strings(&["rev-parse", "--verify", "HEAD"]),
        None,
    )?;
    let head = CommitId::checked(ObjectId::parse(
        repository.identity.object_format(),
        one_line(&output.stdout, Operation::ReopenWorktree)?,
        Operation::ReopenWorktree,
    )?);
    let symbolic = repository.runner.observe(
        &root,
        location,
        CommandAccess::Read,
        Operation::ReopenWorktree,
        &strings(&["symbolic-ref", "-q", "HEAD"]),
        None,
    )?;
    match symbolic.status.code() {
        Some(1) => {}
        Some(0) => return Err(conflict("existing worktree HEAD is not detached")),
        _ => {
            return Err(GitError::command(
                Operation::ReopenWorktree,
                symbolic.status.code(),
                &symbolic.stderr,
            ));
        }
    }
    let baseline = repository.resolve_baseline(&head.to_string())?;
    let registration = RegisteredWorktree {
        repository_digest: repository.identity.digest(),
        name: request.name,
        root,
        git_dir,
        baseline,
        access: request.access,
    };
    let observed = repository.inspect_worktree(&registration)?;
    if observed.head() != registration.baseline.commit() || !observed.is_detached() {
        return Err(GitError::new(
            ErrorKind::ObjectMismatch,
            Operation::ReopenWorktree,
            RecoveryClass::Reconcile,
            "existing worktree changed while its current detached baseline was recovered",
        ));
    }
    Ok(registration)
}

fn validate_existing_destination(
    repository: &GitRepository,
    name: &crate::WorktreeName,
    requested_destination: &std::path::Path,
) -> Result<PathBuf, GitError> {
    let metadata = std::fs::symlink_metadata(requested_destination).map_err(|source| {
        GitError::io(
            Operation::ReopenWorktree,
            RecoveryClass::Reconcile,
            "inspect existing worktree destination",
            source,
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(conflict("existing worktree destination is not a real directory"));
    }
    let parent = requested_destination.parent().ok_or_else(|| {
        GitError::new(
            ErrorKind::InvalidInput,
            Operation::ReopenWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination has no parent directory",
        )
    })?;
    let parent = std::fs::canonicalize(parent).map_err(|source| {
        GitError::io(
            Operation::ReopenWorktree,
            RecoveryClass::CorrectRequest,
            "canonicalize worktree destination parent",
            source,
        )
    })?;
    let leaf = requested_destination.file_name().ok_or_else(|| {
        GitError::new(
            ErrorKind::InvalidInput,
            Operation::ReopenWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination has no final component",
        )
    })?;
    if leaf != name.as_str() {
        return Err(GitError::new(
            ErrorKind::InvalidInput,
            Operation::ReopenWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination leaf must equal the validated name",
        ));
    }
    let destination = parent.join(leaf);
    if destination.starts_with(repository.identity.common_dir())
        || destination.starts_with(repository.identity.git_dir())
        || destination.starts_with(repository.identity.repository_root())
    {
        return Err(conflict("worktree destination is inside protected repository storage"));
    }
    Ok(destination)
}

fn conflict(detail: &'static str) -> GitError {
    GitError::new(
        ErrorKind::WorktreeConflict,
        Operation::ReopenWorktree,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
