//! Structured Git observations anchored to C1 immutable identities.

use peritus_git::{
    CandidateSnapshot, GitDiffObservation, GitHistoryObservation, StatusObservation,
};
use peritus_types::{Generation, RevisionNumber, Sha256Digest, SnapshotId, WorkspaceId};
use peritus_workspace::ReadOnlyWorkspace;

use crate::{
    DiffInput, GitToolError, GitToolErrorKind, GitToolOperation, HistoryInput, RecoveryClass,
    SnapshotInput, StatusInput,
};

/// Exact current C1 immutable snapshot observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotObservation {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    commit: peritus_git::CommitId,
    tree: peritus_git::TreeId,
    digest: Sha256Digest,
}

impl SnapshotObservation {
    /// Returns the owning workspace lineage.
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
    /// Returns the immutable commit.
    #[must_use]
    pub const fn commit(&self) -> peritus_git::CommitId {
        self.commit
    }
    /// Returns the immutable root tree.
    #[must_use]
    pub const fn tree(&self) -> peritus_git::TreeId {
        self.tree
    }
    /// Returns the canonical snapshot observation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Exact retained C1 candidate-snapshot observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedSnapshotObservation {
    workspace_id: WorkspaceId,
    snapshot_id: SnapshotId,
    commit: peritus_git::CommitId,
    tree: peritus_git::TreeId,
    reference: String,
    manifest_digest: Sha256Digest,
}

impl RetainedSnapshotObservation {
    /// Returns the owning workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the stable retained snapshot identity.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the immutable retained commit.
    #[must_use]
    pub const fn commit(&self) -> peritus_git::CommitId {
        self.commit
    }
    /// Returns the immutable retained tree.
    #[must_use]
    pub const fn tree(&self) -> peritus_git::TreeId {
        self.tree
    }
    /// Returns the protected Peritus snapshot reference.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }
    /// Returns the complete candidate-snapshot manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
}

/// Read-only Git service fixed to one C1 immutable snapshot handle.
pub struct GitReadService<'a> {
    workspace: &'a ReadOnlyWorkspace,
}

impl<'a> GitReadService<'a> {
    /// Binds structured Git observations to one checked immutable C1 handle.
    #[must_use]
    pub const fn new(workspace: &'a ReadOnlyWorkspace) -> Self {
        Self { workspace }
    }

    /// Observes exact structured status.
    ///
    /// # Errors
    /// Returns a typed C1 Git observation failure.
    pub fn status(&self, _input: StatusInput) -> Result<StatusObservation, GitToolError> {
        self.workspace.inspect().map_err(|error| git_error(GitToolOperation::Status, &error))
    }

    /// Observes a bounded diff from a resolved baseline to this immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed C1 Git observation failure.
    pub fn diff(&self, input: &DiffInput) -> Result<GitDiffObservation, GitToolError> {
        self.workspace
            .git_diff(&input.base_revision, input.maximum_entries, input.maximum_patch_bytes)
            .map_err(|error| git_error(GitToolOperation::Diff, &error))
    }

    /// Observes bounded history from this immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed C1 Git observation failure.
    pub fn history(&self, input: HistoryInput) -> Result<GitHistoryObservation, GitToolError> {
        self.workspace
            .git_history(input.maximum_commits)
            .map_err(|error| git_error(GitToolOperation::History, &error))
    }

    /// Observes the current immutable C1 snapshot identity.
    #[must_use]
    pub fn current_snapshot(&self) -> SnapshotObservation {
        let snapshot = self.workspace.snapshot();
        let mut bytes = b"PERITUS-GIT-TOOL-SNAPSHOT-V1\0".to_vec();
        bytes.extend_from_slice(snapshot.workspace_id().as_bytes());
        bytes.extend_from_slice(&snapshot.generation().get().to_be_bytes());
        bytes.extend_from_slice(&snapshot.revision().get().to_be_bytes());
        bytes.extend_from_slice(snapshot.commit().object_id().as_bytes());
        bytes.extend_from_slice(snapshot.tree().object_id().as_bytes());
        SnapshotObservation {
            workspace_id: snapshot.workspace_id(),
            generation: snapshot.generation(),
            revision: snapshot.revision(),
            commit: snapshot.commit(),
            tree: snapshot.tree(),
            digest: peritus_codec::sha256(&bytes),
        }
    }

    /// Projects one already-resolved retained C1 candidate snapshot.
    ///
    /// # Errors
    /// Rejects an identity or lineage different from the exact selector/current workspace.
    pub fn retained_snapshot(
        &self,
        input: SnapshotInput,
        retained: &CandidateSnapshot,
    ) -> Result<RetainedSnapshotObservation, GitToolError> {
        let SnapshotInput::Retained(expected) = input else {
            return Err(GitToolError::invalid(
                GitToolOperation::Snapshot,
                "current snapshot selection does not accept retained metadata",
            ));
        };
        if expected != retained.snapshot_id()
            || retained.workspace_id() != self.workspace.snapshot().workspace_id()
        {
            return Err(GitToolError::invalid(
                GitToolOperation::Snapshot,
                "retained snapshot identity or workspace lineage differs",
            ));
        }
        Ok(RetainedSnapshotObservation {
            workspace_id: retained.workspace_id(),
            snapshot_id: retained.snapshot_id(),
            commit: retained.commit(),
            tree: retained.tree(),
            reference: retained.reference().as_str().to_owned(),
            manifest_digest: retained.manifest_digest(),
        })
    }

    /// Reports unavailable branch delivery without invoking Git or mutating a reference.
    ///
    /// # Errors
    /// Always returns the frozen typed unsupported result until C1 owns merge delivery.
    pub const fn merge_unsupported(&self) -> Result<(), GitToolError> {
        Err(GitToolError::new(
            GitToolErrorKind::Unsupported,
            GitToolOperation::Merge,
            RecoveryClass::SelectSupportedOperation,
            "C1 has no authorized merge-delivery operation",
        ))
    }
}

const fn git_error(operation: GitToolOperation, error: &peritus_git::GitError) -> GitToolError {
    let recovery = match error.recovery() {
        peritus_git::RecoveryClass::CorrectRequest => RecoveryClass::CorrectInput,
        peritus_git::RecoveryClass::Reobserve | peritus_git::RecoveryClass::Retry => {
            RecoveryClass::Reobserve
        }
        peritus_git::RecoveryClass::Reconcile | peritus_git::RecoveryClass::Quarantine => {
            RecoveryClass::Reconcile
        }
    };
    GitToolError::new(
        GitToolErrorKind::Git,
        operation,
        recovery,
        "structured C1 Git observation failed",
    )
}
