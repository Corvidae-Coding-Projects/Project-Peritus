//! Explicit boundaries for independently owned nested repositories.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::command::{CommandAccess, one_line};
use crate::repository::strings;
use crate::{GitError, GitRepository, Operation, RegisteredWorktree, RepositoryIdentity, TreeId};

/// A nested repository identity bound to one exact parent worktree.
///
/// Parent candidates capture its committed HEAD as a gitlink. Its working files and history
/// remain independently owned; parent restoration never recursively restores those files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredNestedRepository {
    parent: PathBuf,
    repository: peritus_types::Sha256Digest,
    identity: RepositoryIdentity,
}

impl RegisteredNestedRepository {
    /// Returns the independently owned nested repository root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.identity.repository_root()
    }
}

impl GitRepository {
    /// Registers an already opened nested repository without acquiring its content ownership.
    ///
    /// # Errors
    /// Rejects stale registrations, bare repositories, and roots outside the parent worktree.
    pub fn register_nested_repository(
        &self,
        worktree: &RegisteredWorktree,
        nested: &Self,
    ) -> Result<RegisteredNestedRepository, GitError> {
        self.validate_registration(worktree, Operation::InspectWorktree)?;
        let root = nested.identity.repository_root();
        if nested.identity.is_bare()
            || root == worktree.root()
            || !root.starts_with(worktree.root())
            || root.starts_with(worktree.git_dir())
        {
            return Err(conflict(
                Operation::InspectWorktree,
                "nested repository is outside the parent content root",
            ));
        }
        let registration = RegisteredNestedRepository {
            parent: worktree.root().to_owned(),
            repository: self.identity.digest(),
            identity: nested.identity.clone(),
        };
        validate(self, worktree, std::slice::from_ref(&registration), Operation::InspectWorktree)?;
        Ok(registration)
    }
}

pub(super) fn validate(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    registrations: &[RegisteredNestedRepository],
    operation: Operation,
) -> Result<(), GitError> {
    for (index, registration) in registrations.iter().enumerate() {
        check_cancelled(repository, operation)?;
        if registration.parent != worktree.root()
            || registration.repository != repository.identity.digest()
            || registrations[..index].iter().any(|other| {
                registration.root().starts_with(other.root())
                    || other.root().starts_with(registration.root())
            })
        {
            return Err(conflict(
                operation,
                "nested registration does not uniquely bind this parent",
            ));
        }
        for (argument, expected) in [
            ("--show-toplevel", registration.root()),
            ("--absolute-git-dir", registration.identity.git_dir()),
            ("--git-common-dir", registration.identity.common_dir()),
        ] {
            let output = repository.runner.checked(
                registration.root(),
                None,
                CommandAccess::Read,
                operation,
                &strings(&["rev-parse", "--path-format=absolute", argument]),
                None,
            )?;
            let observed =
                std::fs::canonicalize(one_line(&output.stdout, operation)?).map_err(|source| {
                    GitError::io(
                        operation,
                        crate::RecoveryClass::Reconcile,
                        "inspect nested repository identity",
                        source,
                    )
                })?;
            if observed != expected {
                return Err(conflict(operation, "nested repository identity changed"));
            }
        }
    }
    protect_index(repository, worktree, registrations, operation)
}

fn protect_index(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    registrations: &[RegisteredNestedRepository],
    operation: Operation,
) -> Result<(), GitError> {
    if registrations.is_empty() {
        return Ok(());
    }
    let output = repository.runner.checked(
        worktree.root(),
        Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
        CommandAccess::Read,
        operation,
        &strings(&["ls-files", "--stage", "-z"]),
        None,
    )?;
    protect_entries(repository, worktree, registrations, &output.stdout, operation)
}

#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "Git revision syntax peels HEAD to a commit"
)]
pub(super) fn candidate_links(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    registrations: &[RegisteredNestedRepository],
) -> Result<Vec<(PathBuf, crate::ObjectId)>, GitError> {
    registrations
        .iter()
        .map(|registration| {
            let operation = Operation::CreateCandidate;
            check_cancelled(repository, operation)?;
            let output = repository.runner.checked(
                registration.root(),
                None,
                CommandAccess::Read,
                operation,
                &strings(&["rev-parse", "--verify", "HEAD^{commit}"]),
                None,
            )?;
            let commit = crate::ObjectId::parse(
                repository.identity.object_format(),
                one_line(&output.stdout, operation)?,
                operation,
            )?;
            let relative = registration
                .root()
                .strip_prefix(worktree.root())
                .map_err(|_| conflict(operation, "nested root escaped parent"))?
                .to_owned();
            Ok((relative, commit))
        })
        .collect()
}

pub(super) fn stage_links(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    links: &[(PathBuf, crate::ObjectId)],
) -> Result<(), GitError> {
    for (path, commit) in links {
        check_cancelled(repository, Operation::CreateCandidate)?;
        repository.runner.checked(
            worktree.root(),
            Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Write,
            Operation::CreateCandidate,
            &[
                OsString::from("update-index"),
                OsString::from("--add"),
                OsString::from("--cacheinfo"),
                OsString::from("160000"),
                OsString::from(commit.to_string()),
                path.as_os_str().to_owned(),
            ],
            None,
        )?;
    }
    Ok(())
}

pub(super) fn protect_restore(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    registrations: &[RegisteredNestedRepository],
    tree: TreeId,
) -> Result<(), GitError> {
    if registrations.is_empty() {
        return Ok(());
    }
    let operation = Operation::RestoreSnapshot;
    let output = repository.checked_repo_command(
        operation,
        CommandAccess::Read,
        &[OsString::from("ls-tree"), OsString::from("-rz"), OsString::from(tree.to_string())],
        None,
    )?;
    protect_entries(repository, worktree, registrations, &output.stdout, operation)
}

fn protect_entries(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    registrations: &[RegisteredNestedRepository],
    entries: &[u8],
    operation: Operation,
) -> Result<(), GitError> {
    for record in entries.split(|byte| *byte == 0).filter(|entry| !entry.is_empty()) {
        check_cancelled(repository, operation)?;
        let (header, path) = record.split_at(
            record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| conflict(operation, "malformed snapshot tree entry"))?,
        );
        let path = super::support::inventory_path(&path[1..], operation)?;
        for registration in registrations {
            let relative = registration
                .root()
                .strip_prefix(worktree.root())
                .map_err(|_| conflict(operation, "nested root escaped parent"))?;
            let overlaps = path.starts_with(relative) || relative.starts_with(&path);
            let exact_gitlink = path == relative && header.starts_with(b"160000 ");
            if overlaps && !exact_gitlink {
                return Err(conflict(
                    operation,
                    "parent content collides with independently owned nested repository",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn check_cancelled(
    repository: &GitRepository,
    operation: Operation,
) -> Result<(), GitError> {
    if repository.runner.cancellation.is_cancelled() {
        Err(GitError::new(
            crate::ErrorKind::Cancelled,
            operation,
            crate::RecoveryClass::Reconcile,
            "worktree ownership inspection was cancelled",
        ))
    } else {
        Ok(())
    }
}

fn conflict(operation: Operation, detail: &'static str) -> GitError {
    GitError::new(
        crate::ErrorKind::WorktreeConflict,
        operation,
        crate::RecoveryClass::Reconcile,
        detail,
    )
}
