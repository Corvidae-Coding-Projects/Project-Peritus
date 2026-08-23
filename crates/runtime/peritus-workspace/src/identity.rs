//! Exact workspace, resource, root, lineage, and snapshot identity values.

use std::path::{Path, PathBuf};

use peritus_git::{CommitId, TreeId};
use peritus_types::{EnvironmentId, Generation, ResourceId, RevisionNumber, WorkspaceId};

/// Durable nominal and filesystem binding for one writable workspace lineage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceBinding {
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    root: PathBuf,
    baseline_commit: CommitId,
    baseline_tree: TreeId,
}

impl WorkspaceBinding {
    /// Creates a binding from a canonical absolute opened root and immutable Git baseline.
    ///
    /// # Errors
    ///
    /// Rejects relative roots. The open boundary separately compares this root with the registered
    /// worktree's canonical root.
    pub fn new(
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        root: PathBuf,
        baseline_commit: CommitId,
        baseline_tree: TreeId,
    ) -> Result<Self, crate::WorkspaceError> {
        if !root.is_absolute() {
            return Err(crate::WorkspaceError::new(
                crate::ErrorCode::InvalidInput,
                crate::WorkspaceOperation::Open,
                crate::RecoveryClass::CorrectRequest,
                "workspace root must be absolute",
            ));
        }
        Ok(Self { workspace_id, resource_id, environment_id, root, baseline_commit, baseline_tree })
    }

    /// Returns the exact workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the exact authorized resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    /// Returns the exact environment identity.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the opened canonical root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Returns the immutable lineage baseline commit.
    #[must_use]
    pub const fn baseline_commit(&self) -> CommitId {
        self.baseline_commit
    }
    /// Returns the immutable lineage baseline tree.
    #[must_use]
    pub const fn baseline_tree(&self) -> TreeId {
        self.baseline_tree
    }
}

/// Immutable snapshot identity within one workspace lineage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotIdentity {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    commit: CommitId,
    tree: TreeId,
}

impl SnapshotIdentity {
    /// Creates an exact immutable snapshot identity.
    #[must_use]
    pub const fn new(
        workspace_id: WorkspaceId,
        generation: Generation,
        revision: RevisionNumber,
        commit: CommitId,
        tree: TreeId,
    ) -> Self {
        Self { workspace_id, generation, revision, commit, tree }
    }

    /// Returns the owning lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the fenced generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    /// Returns the logical revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// Returns the immutable snapshot commit.
    #[must_use]
    pub const fn commit(&self) -> CommitId {
        self.commit
    }
    /// Returns the immutable content tree.
    #[must_use]
    pub const fn tree(&self) -> TreeId {
        self.tree
    }
}
