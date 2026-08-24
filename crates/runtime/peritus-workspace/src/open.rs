//! Ownership-bearing open requests for writable and read-only workspace handles.

use std::path::PathBuf;

use peritus_git::{GitRepository, RegisteredWorktree};

use crate::{SnapshotIdentity, WorkspaceBinding, WorkspaceState};

/// Complete inputs needed to open one move-only writable workspace.
pub struct WritableOpenRequest {
    repository: GitRepository,
    worktree: RegisteredWorktree,
    state: WorkspaceState,
    transaction_root: PathBuf,
}

impl WritableOpenRequest {
    /// Creates an unprivileged open request. [`crate::WritableWorkspace::open`] validates it.
    #[must_use]
    pub fn new(
        repository: GitRepository,
        worktree: RegisteredWorktree,
        state: WorkspaceState,
        transaction_root: impl Into<PathBuf>,
    ) -> Self {
        Self { repository, worktree, state, transaction_root: transaction_root.into() }
    }

    pub(crate) fn into_parts(self) -> (GitRepository, RegisteredWorktree, WorkspaceState, PathBuf) {
        (self.repository, self.worktree, self.state, self.transaction_root)
    }
}

/// Complete inputs needed to open one immutable read-only snapshot worktree.
pub struct ReadOnlyOpenRequest {
    repository: GitRepository,
    worktree: RegisteredWorktree,
    snapshot: SnapshotIdentity,
    writer_root: PathBuf,
    binding: Option<WorkspaceBinding>,
}

impl ReadOnlyOpenRequest {
    /// Creates an unprivileged request whose distinct root and immutable IDs are checked on open.
    #[must_use]
    pub fn new(
        repository: GitRepository,
        worktree: RegisteredWorktree,
        snapshot: SnapshotIdentity,
        writer_root: impl Into<PathBuf>,
    ) -> Self {
        Self { repository, worktree, snapshot, writer_root: writer_root.into(), binding: None }
    }

    /// Adds the actual writable lineage binding from which C4 read target identity is derived.
    #[must_use]
    pub fn with_workspace_binding(mut self, binding: WorkspaceBinding) -> Self {
        self.binding = Some(binding);
        self
    }

    pub(crate) fn into_parts(
        self,
    ) -> (GitRepository, RegisteredWorktree, SnapshotIdentity, PathBuf, Option<WorkspaceBinding>)
    {
        (self.repository, self.worktree, self.snapshot, self.writer_root, self.binding)
    }
}
