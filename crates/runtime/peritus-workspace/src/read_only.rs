//! Immutable snapshot worktree with inspection-only methods.

use std::path::Path;

use peritus_git::{GitRepository, RegisteredWorktree, StatusObservation, WorktreeAccess};

use crate::{
    ErrorCode, ReadOnlyOpenRequest, ReadOnlyTargetBinding, RecoveryClass, SnapshotIdentity,
    WorkspaceError, WorkspaceOperation,
};

/// A separate detached worktree fixed to one immutable snapshot.
pub struct ReadOnlyWorkspace {
    repository: GitRepository,
    worktree: RegisteredWorktree,
    snapshot: SnapshotIdentity,
    target: Option<ReadOnlyTargetBinding>,
}

impl ReadOnlyWorkspace {
    /// Opens and revalidates a detached read-only snapshot worktree.
    ///
    /// # Errors
    ///
    /// Rejects writable access, a shared writer root, repository drift, or snapshot OID drift.
    pub fn open(request: ReadOnlyOpenRequest) -> Result<Self, WorkspaceError> {
        let (repository, worktree, snapshot, writer_root, binding) = request.into_parts();
        let valid = worktree.access() == WorktreeAccess::ReadOnly
            && worktree.repository_digest() == repository.identity().digest()
            && worktree.root() != writer_root
            && worktree.baseline().commit() == snapshot.commit()
            && worktree.baseline().tree() == snapshot.tree();
        if !valid {
            return Err(open_error(
                "read-only worktree is shared, writable, or snapshot-mismatched",
            ));
        }
        let observed = repository
            .inspect_worktree(&worktree)
            .map_err(|_| open_error("registered read-only worktree failed revalidation"))?;
        if !observed.is_detached() || observed.head() != snapshot.commit() {
            return Err(open_error("read-only worktree is not fixed to the immutable snapshot"));
        }
        if binding.as_ref().is_some_and(|binding| {
            binding.workspace_id() != snapshot.workspace_id() || binding.root() != writer_root
        }) {
            return Err(open_error("read-only target belongs to another workspace lineage"));
        }
        let target = binding.as_ref().map(|binding| {
            ReadOnlyTargetBinding::new(
                binding.workspace_id(),
                binding.environment_id(),
                binding.resource_id(),
            )
        });
        Ok(Self { repository, worktree, snapshot, target })
    }

    /// Returns the immutable snapshot identity.
    #[must_use]
    pub const fn snapshot(&self) -> &SnapshotIdentity {
        &self.snapshot
    }
    /// Returns the physically distinct read-only worktree root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.worktree.root()
    }
    /// Returns the exact C4-readable target binding when it was supplied at open.
    #[must_use]
    pub const fn target_binding(&self) -> Option<ReadOnlyTargetBinding> {
        self.target
    }
    /// Re-observes exact Git status without exposing mutation operations.
    ///
    /// # Errors
    ///
    /// Returns a structured Git failure when the worktree cannot be inspected exactly.
    pub fn inspect(&self) -> Result<StatusObservation, peritus_git::GitError> {
        self.repository.status(&self.worktree)
    }

    pub(crate) const fn repository(&self) -> &GitRepository {
        &self.repository
    }

    pub(crate) const fn worktree(&self) -> &RegisteredWorktree {
        &self.worktree
    }
}

const fn open_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::InvalidInput,
        WorkspaceOperation::Open,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

/// Compile-time API check: the read-only type has no patch/candidate/rollback method.
///
/// ```compile_fail
/// fn mutate(snapshot: &mut peritus_workspace::ReadOnlyWorkspace) {
///     snapshot.apply_patch();
/// }
/// ```
const _: () = ();
