//! Move-only writable workspace handle hidden behind the target gateway.

use std::path::{Path, PathBuf};

use peritus_git::{
    GitRepository, ReconcileDisposition, ReconcileExpectation, RegisteredWorktree, WorktreeAccess,
};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation, WorkspaceState,
    WritableOpenRequest,
};

/// Checked live writable workspace. It intentionally implements neither `Clone` nor `Copy`.
pub struct WritableWorkspace {
    repository: GitRepository,
    worktree: RegisteredWorktree,
    state: WorkspaceState,
    transaction_root: PathBuf,
}

impl WritableWorkspace {
    /// Opens a registered detached writable worktree bound to exact durable state.
    ///
    /// # Errors
    ///
    /// Rejects repository, root, access, baseline, or transaction-root mismatches.
    pub fn open(request: WritableOpenRequest) -> Result<Self, WorkspaceError> {
        let (repository, worktree, state, transaction_root) = request.into_parts();
        let binding = state.binding();
        if worktree.access() != WorktreeAccess::Writable
            || worktree.repository_digest() != repository.identity().digest()
            || worktree.root() != binding.root()
            || worktree.baseline().commit() != binding.baseline_commit()
            || worktree.baseline().tree() != binding.baseline_tree()
        {
            return Err(open_error("writable worktree does not match its exact binding"));
        }
        let observed = repository
            .inspect_worktree(&worktree)
            .map_err(|_| open_error("registered writable worktree failed revalidation"))?;
        if !observed.is_detached() || observed.head() != binding.baseline_commit() {
            return Err(open_error("writable worktree is not at its exact detached baseline HEAD"));
        }
        if state.condition() == crate::WorkspaceCondition::Clean {
            let reconciled = repository
                .reconcile(ReconcileExpectation::new(
                    &worktree,
                    binding.baseline_commit(),
                    state.current_snapshot().tree(),
                ))
                .map_err(|_| open_error("clean workspace could not be reconciled exactly"))?;
            if !matches!(reconciled.disposition(), ReconcileDisposition::Clean) {
                return Err(open_error(
                    "clean workspace differs from its exact current snapshot tree",
                ));
            }
        }
        let transaction_root = crate::transaction_namespace::open(
            transaction_root,
            &state,
            worktree.root(),
            repository.identity().common_dir(),
        )?;
        let mut workspace = Self { repository, worktree, state, transaction_root };
        workspace.restore_action_consumption()?;
        Ok(workspace)
    }

    /// Returns immutable current state.
    #[must_use]
    pub const fn state(&self) -> &WorkspaceState {
        &self.state
    }
    /// Returns the canonical writable root for diagnostics and checked planning.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.worktree.root()
    }

    /// Returns the canonical target-owned transaction namespace used for recovery evidence.
    #[must_use]
    pub fn transaction_namespace(&self) -> &Path {
        &self.transaction_root
    }

    pub(crate) const fn repository(&self) -> &GitRepository {
        &self.repository
    }
    pub(crate) const fn worktree(&self) -> &RegisteredWorktree {
        &self.worktree
    }
    pub(crate) const fn state_mut(&mut self) -> &mut WorkspaceState {
        &mut self.state
    }
    pub(crate) fn transaction_root(&self) -> &Path {
        &self.transaction_root
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

/// Compile-time API check: writable handles cannot be cloned or copied.
///
/// ```compile_fail
/// fn require_clone<T: Clone>() {}
/// require_clone::<peritus_workspace::WritableWorkspace>();
/// ```
///
/// ```compile_fail
/// fn require_copy<T: Copy>() {}
/// require_copy::<peritus_workspace::WritableWorkspace>();
/// ```
const _: () = ();
