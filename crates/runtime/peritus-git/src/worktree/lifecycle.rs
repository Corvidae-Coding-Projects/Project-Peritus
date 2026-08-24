//! Validated creation, observation, and exact removal effects.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::command::{CommandAccess, git_path, one_line};
use crate::repository::strings;
use crate::{CommitId, ErrorKind, GitError, GitRepository, ObjectId, Operation, RecoveryClass};

use super::{CreateWorktree, RegisteredWorktree, RemovalPolicy, WorktreeObservation};

impl GitRepository {
    /// Creates an exact detached linked worktree at a previously absent destination.
    ///
    /// # Errors
    ///
    /// Rejects conflicts, protected locations, Git failures, or a mismatched resulting worktree.
    pub fn create_worktree(&self, request: CreateWorktree) -> Result<RegisteredWorktree, GitError> {
        self.reject_external_filters(Operation::CreateWorktree, self.common_location())?;
        let destination = validate_destination(self, &request)?;
        let arguments = vec![
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("--detach"),
            git_path(&destination),
            OsString::from(request.baseline.commit().to_string()),
        ];
        self.checked_repo_command(
            Operation::CreateWorktree,
            CommandAccess::Write,
            &arguments,
            None,
        )?;
        let root = std::fs::canonicalize(&destination).map_err(|source| {
            GitError::io(
                Operation::CreateWorktree,
                RecoveryClass::Reconcile,
                "canonicalize newly created worktree",
                source,
            )
        })?;
        let git_dir = discover_worktree_git_dir(self, &root, Operation::CreateWorktree)?;
        let registration = RegisteredWorktree {
            repository_digest: self.identity.digest(),
            name: request.name,
            root,
            git_dir,
            baseline: request.baseline,
            access: request.access,
        };
        let observed = self.inspect_worktree(&registration)?;
        if observed.head != registration.baseline.commit() || !observed.detached {
            return Err(GitError::new(
                ErrorKind::ObjectMismatch,
                Operation::CreateWorktree,
                RecoveryClass::Reconcile,
                "created worktree does not have the exact detached baseline",
            ));
        }
        Ok(registration)
    }

    /// Revalidates the root, Git directory, repository binding, HEAD, and detached state.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict or protocol failure if the registration has drifted.
    pub fn inspect_worktree(
        &self,
        worktree: &RegisteredWorktree,
    ) -> Result<WorktreeObservation, GitError> {
        self.validate_registration(worktree, Operation::InspectWorktree)?;
        let location = Some(Self::worktree_location(&worktree.root, &worktree.git_dir));
        let output = self.runner.checked(
            &worktree.root,
            location,
            CommandAccess::Read,
            Operation::InspectWorktree,
            &strings(&["rev-parse", "--verify", "HEAD"]),
            None,
        )?;
        let head = CommitId::checked(ObjectId::parse(
            self.identity.object_format(),
            one_line(&output.stdout, Operation::InspectWorktree)?,
            Operation::InspectWorktree,
        )?);
        let symbolic = self.runner.observe(
            &worktree.root,
            location,
            CommandAccess::Read,
            Operation::InspectWorktree,
            &strings(&["symbolic-ref", "-q", "HEAD"]),
            None,
        )?;
        let detached = match symbolic.status.code() {
            Some(0) => false,
            Some(1) => true,
            _ => {
                return Err(GitError::command(
                    Operation::InspectWorktree,
                    symbolic.status.code(),
                    &symbolic.stderr,
                ));
            }
        };
        Ok(WorktreeObservation {
            root: worktree.root.clone(),
            git_dir: worktree.git_dir.clone(),
            head,
            detached,
        })
    }

    /// Removes only the exact registered worktree path.
    ///
    /// # Errors
    ///
    /// Refuses registration drift and, under [`RemovalPolicy::RequireClean`], any status entry.
    pub fn remove_worktree(
        &self,
        worktree: &RegisteredWorktree,
        policy: RemovalPolicy,
    ) -> Result<(), GitError> {
        self.validate_registration(worktree, Operation::RemoveWorktree)?;
        if policy == RemovalPolicy::RequireClean && !self.status(worktree)?.is_clean() {
            return Err(GitError::new(
                ErrorKind::DirtyWorktree,
                Operation::RemoveWorktree,
                RecoveryClass::CorrectRequest,
                "registered worktree contains status entries",
            ));
        }
        let mut arguments = strings(&["worktree", "remove"]);
        if policy == RemovalPolicy::ForceRegistered {
            arguments.push(OsString::from("--force"));
        }
        arguments.push(git_path(&worktree.root));
        self.checked_repo_command(
            Operation::RemoveWorktree,
            CommandAccess::Write,
            &arguments,
            None,
        )?;
        if worktree.root.exists() {
            return Err(GitError::new(
                ErrorKind::Indeterminate,
                Operation::RemoveWorktree,
                RecoveryClass::Reconcile,
                "Git reported removal but the registered root still exists",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_registration(
        &self,
        worktree: &RegisteredWorktree,
        operation: Operation,
    ) -> Result<(), GitError> {
        if worktree.repository_digest != self.identity.digest() {
            return Err(GitError::new(
                ErrorKind::WorktreeConflict,
                operation,
                RecoveryClass::CorrectRequest,
                "worktree registration belongs to another repository",
            ));
        }
        let root = std::fs::canonicalize(&worktree.root).map_err(|source| {
            GitError::io(
                operation,
                RecoveryClass::Reconcile,
                "canonicalize registered worktree root",
                source,
            )
        })?;
        let git_dir = discover_worktree_git_dir(self, &root, operation)?;
        if root != worktree.root || git_dir != worktree.git_dir {
            return Err(GitError::new(
                ErrorKind::WorktreeConflict,
                operation,
                RecoveryClass::Quarantine,
                "worktree registration no longer matches filesystem and Git metadata",
            ));
        }
        Ok(())
    }
}

fn validate_destination(
    repository: &GitRepository,
    request: &CreateWorktree,
) -> Result<PathBuf, GitError> {
    match std::fs::symlink_metadata(&request.destination) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(GitError::new(
                ErrorKind::WorktreeConflict,
                Operation::CreateWorktree,
                RecoveryClass::CorrectRequest,
                "worktree destination already exists",
            ));
        }
        Err(source) => {
            return Err(GitError::io(
                Operation::CreateWorktree,
                RecoveryClass::CorrectRequest,
                "inspect worktree destination",
                source,
            ));
        }
    }
    let parent = request.destination.parent().ok_or_else(|| {
        GitError::new(
            ErrorKind::InvalidInput,
            Operation::CreateWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination has no parent directory",
        )
    })?;
    let parent = std::fs::canonicalize(parent).map_err(|source| {
        GitError::io(
            Operation::CreateWorktree,
            RecoveryClass::CorrectRequest,
            "canonicalize worktree destination parent",
            source,
        )
    })?;
    let leaf = request.destination.file_name().ok_or_else(|| {
        GitError::new(
            ErrorKind::InvalidInput,
            Operation::CreateWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination has no final component",
        )
    })?;
    if leaf != request.name.as_str() {
        return Err(GitError::new(
            ErrorKind::InvalidInput,
            Operation::CreateWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination leaf must equal the validated name",
        ));
    }
    let destination = parent.join(leaf);
    if destination.starts_with(repository.identity.common_dir())
        || destination.starts_with(repository.identity.git_dir())
        || destination.starts_with(repository.identity.repository_root())
    {
        return Err(GitError::new(
            ErrorKind::WorktreeConflict,
            Operation::CreateWorktree,
            RecoveryClass::CorrectRequest,
            "worktree destination is inside protected repository storage",
        ));
    }
    ensure_unregistered(repository, &destination)?;
    Ok(destination)
}

fn ensure_unregistered(repository: &GitRepository, destination: &Path) -> Result<(), GitError> {
    let output = repository.checked_repo_command(
        Operation::CreateWorktree,
        CommandAccess::Read,
        &strings(&["worktree", "list", "--porcelain", "-z"]),
        None,
    )?;
    for record in output.stdout.split(|byte| *byte == 0).filter(|record| !record.is_empty()) {
        let Some(value) = record.strip_prefix(b"worktree ") else {
            continue;
        };
        let value = std::str::from_utf8(value).map_err(|_| {
            GitError::new(
                ErrorKind::GitProtocol,
                Operation::CreateWorktree,
                RecoveryClass::Quarantine,
                "Git reported a non-UTF-8 registered worktree path",
            )
        })?;
        if Path::new(value) == destination {
            return Err(GitError::new(
                ErrorKind::WorktreeConflict,
                Operation::CreateWorktree,
                RecoveryClass::Reconcile,
                "worktree destination already has a Git registration",
            ));
        }
    }
    Ok(())
}

pub(super) fn discover_worktree_git_dir(
    repository: &GitRepository,
    root: &Path,
    operation: Operation,
) -> Result<PathBuf, GitError> {
    let output = repository.runner.checked(
        root,
        None,
        CommandAccess::Read,
        operation,
        &strings(&["rev-parse", "--path-format=absolute", "--absolute-git-dir"]),
        None,
    )?;
    let git_dir = PathBuf::from(one_line(&output.stdout, operation)?);
    let git_dir = std::fs::canonicalize(git_dir).map_err(|source| {
        GitError::io(
            operation,
            RecoveryClass::Reconcile,
            "canonicalize Git-reported worktree metadata",
            source,
        )
    })?;
    let common_output = repository.runner.checked(
        root,
        None,
        CommandAccess::Read,
        operation,
        &strings(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
        None,
    )?;
    let common = std::fs::canonicalize(PathBuf::from(one_line(&common_output.stdout, operation)?))
        .map_err(|source| {
            GitError::io(
                operation,
                RecoveryClass::Reconcile,
                "canonicalize worktree common Git directory",
                source,
            )
        })?;
    if common != repository.identity.common_dir() {
        return Err(GitError::new(
            ErrorKind::WorktreeConflict,
            operation,
            RecoveryClass::Quarantine,
            "worktree belongs to another common Git directory",
        ));
    }
    Ok(git_dir)
}
