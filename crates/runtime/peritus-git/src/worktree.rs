//! Detached linked-worktree registrations and lifecycle operations.

mod lifecycle;
mod recovery;

use std::path::{Path, PathBuf};

use peritus_types::Sha256Digest;

use crate::{Baseline, CommitId, GitError, GitRepository, WorktreeName};

pub fn recover_existing(
    repository: &GitRepository,
    request: CreateWorktree,
) -> Result<RegisteredWorktree, GitError> {
    recovery::recover_existing(repository, request)
}

pub fn recover_current(
    repository: &GitRepository,
    request: RecoverWorktree,
) -> Result<RegisteredWorktree, GitError> {
    recovery::recover_current(repository, request)
}

/// Intended access class for a managed worktree.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorktreeAccess {
    /// The workspace gateway may use the worktree for authorized mutations.
    Writable,
    /// The worktree is intended only for inspection of an immutable snapshot.
    ReadOnly,
}

/// Checked request to create one detached linked worktree.
#[derive(Clone, Debug)]
pub struct CreateWorktree {
    name: WorktreeName,
    destination: PathBuf,
    baseline: Baseline,
    access: WorktreeAccess,
}

impl CreateWorktree {
    /// Creates a worktree request with an explicit portable name and destination.
    #[must_use]
    pub fn new(
        name: WorktreeName,
        destination: impl Into<PathBuf>,
        baseline: Baseline,
        access: WorktreeAccess,
    ) -> Self {
        Self { name, destination: destination.into(), baseline, access }
    }
}

/// Request to re-register a previously trusted detached worktree at its current HEAD.
///
/// Unlike [`CreateWorktree`], this request deliberately has no caller-supplied baseline. Recovery
/// observes and validates the existing linked worktree before adopting its current detached commit
/// and tree. Working-tree changes are retained.
#[derive(Clone, Debug)]
pub struct RecoverWorktree {
    name: WorktreeName,
    destination: PathBuf,
    access: WorktreeAccess,
}

impl RecoverWorktree {
    /// Creates a recovery request for an existing, previously trusted managed worktree.
    #[must_use]
    pub fn new(
        name: WorktreeName,
        destination: impl Into<PathBuf>,
        access: WorktreeAccess,
    ) -> Self {
        Self { name, destination: destination.into(), access }
    }
}

/// Repository-bound registration returned only after Git and filesystem validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredWorktree {
    repository_digest: Sha256Digest,
    name: WorktreeName,
    root: PathBuf,
    git_dir: PathBuf,
    baseline: Baseline,
    access: WorktreeAccess,
}

impl RegisteredWorktree {
    #[allow(clippy::redundant_pub_crate)] // Private sibling decoder needs checked construction.
    pub(crate) const fn checked(
        repository_digest: Sha256Digest,
        name: WorktreeName,
        root: PathBuf,
        git_dir: PathBuf,
        baseline: Baseline,
        access: WorktreeAccess,
    ) -> Self {
        Self { repository_digest, name, root, git_dir, baseline, access }
    }

    /// Returns the repository identity to which this registration belongs.
    #[must_use]
    pub const fn repository_digest(&self) -> Sha256Digest {
        self.repository_digest
    }
    /// Returns the caller-selected portable registration name.
    #[must_use]
    pub const fn name(&self) -> &WorktreeName {
        &self.name
    }
    /// Returns the exact canonical worktree root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Returns the exact canonical per-worktree Git directory.
    #[must_use]
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }
    /// Returns the immutable baseline used at creation.
    #[must_use]
    pub const fn baseline(&self) -> Baseline {
        self.baseline
    }
    /// Returns the intended access class.
    #[must_use]
    pub const fn access(&self) -> WorktreeAccess {
        self.access
    }
}

/// Current checked state of a registered detached worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeObservation {
    root: PathBuf,
    git_dir: PathBuf,
    head: CommitId,
    detached: bool,
}

impl WorktreeObservation {
    /// Returns the re-observed canonical worktree root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Returns the re-observed canonical per-worktree Git directory.
    #[must_use]
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }
    /// Returns the current exact HEAD commit.
    #[must_use]
    pub const fn head(&self) -> CommitId {
        self.head
    }
    /// Returns whether HEAD is detached.
    #[must_use]
    pub const fn is_detached(&self) -> bool {
        self.detached
    }
}

/// Explicit cleanup policy for a registered worktree.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RemovalPolicy {
    /// Refuse removal whenever status contains any entry, including ignored files.
    RequireClean,
    /// Ask Git to remove the exact registered path even when it is dirty.
    ForceRegistered,
}
