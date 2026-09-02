//! Candidate-tree creation, retained snapshots, and exact tree restoration.

mod operations;
mod quarantine;
mod support;

use std::path::PathBuf;

use peritus_types::{Sha256Digest, SnapshotId, WorkspaceId};

use crate::{
    Baseline, CandidateSnapshotManifest, CandidateTreeManifest, CommitId, RegisteredWorktree,
    StatusObservation, TreeId,
};

/// Returns the deterministic retained reference for one workspace snapshot identity.
#[must_use]
pub fn expected_snapshot_ref(workspace_id: WorkspaceId, snapshot_id: SnapshotId) -> SnapshotRef {
    support::snapshot_ref(workspace_id, snapshot_id)
}

/// Request to stage and write the complete current worktree result.
#[derive(Clone, Copy, Debug)]
pub struct CandidateRequest<'a> {
    worktree: &'a RegisteredWorktree,
    expected_head: CommitId,
}

impl<'a> CandidateRequest<'a> {
    /// Binds candidate creation to one registration and exact observed HEAD.
    #[must_use]
    pub const fn new(worktree: &'a RegisteredWorktree, expected_head: CommitId) -> Self {
        Self { worktree, expected_head }
    }
}

/// Content-addressed tree plus canonical observations used to construct it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateTree {
    repository_digest: Sha256Digest,
    worktree_root: PathBuf,
    baseline: Baseline,
    head: CommitId,
    tree: TreeId,
    manifest: CandidateTreeManifest,
    prior_status: StatusObservation,
    status: StatusObservation,
}

impl CandidateTree {
    /// Returns the exact Git tree written from the isolated worktree index.
    #[must_use]
    pub const fn tree(&self) -> TreeId {
        self.tree
    }
    /// Returns the unchanged detached HEAD under which the candidate was made.
    #[must_use]
    pub const fn head(&self) -> CommitId {
        self.head
    }
    /// Returns the immutable lineage baseline.
    #[must_use]
    pub const fn baseline(&self) -> Baseline {
        self.baseline
    }
    /// Returns the canonical candidate manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest.digest()
    }
    /// Returns the canonical versioned candidate manifest.
    #[must_use]
    pub const fn manifest(&self) -> &CandidateTreeManifest {
        &self.manifest
    }
    /// Returns status immediately before complete staging.
    #[must_use]
    pub const fn prior_status(&self) -> &StatusObservation {
        &self.prior_status
    }
    /// Returns status immediately after staging and writing the tree.
    #[must_use]
    pub const fn status(&self) -> &StatusObservation {
        &self.status
    }
}

/// Validated namespaced reference owned by the Peritus snapshot subsystem.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SnapshotRef(String);

impl SnapshotRef {
    /// Returns the canonical active or quarantine reference name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Request to create a deterministic snapshot commit and retain it by reference.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotRequest<'a> {
    worktree: &'a RegisteredWorktree,
    candidate: &'a CandidateTree,
    workspace_id: WorkspaceId,
    snapshot_id: SnapshotId,
    parent: CommitId,
}

impl<'a> SnapshotRequest<'a> {
    /// Binds a candidate to its workspace lineage, stable snapshot identity, and parent commit.
    #[must_use]
    pub const fn new(
        worktree: &'a RegisteredWorktree,
        candidate: &'a CandidateTree,
        workspace_id: WorkspaceId,
        snapshot_id: SnapshotId,
        parent: CommitId,
    ) -> Self {
        Self { worktree, candidate, workspace_id, snapshot_id, parent }
    }
}

/// Immutable retained snapshot object and lineage identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateSnapshot {
    manifest: CandidateSnapshotManifest,
}

/// Durable evidence that a divergent retained snapshot was removed from active use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotQuarantine {
    active_reference: SnapshotRef,
    quarantine_reference: SnapshotRef,
    observed_commit: CommitId,
}

impl SnapshotQuarantine {
    /// Returns the active reference that is now absent.
    #[must_use]
    pub const fn active_reference(&self) -> &SnapshotRef {
        &self.active_reference
    }

    /// Returns the reference retaining the divergent value for inspection.
    #[must_use]
    pub const fn quarantine_reference(&self) -> &SnapshotRef {
        &self.quarantine_reference
    }

    /// Returns the divergent commit formerly reachable through the active reference.
    #[must_use]
    pub const fn observed_commit(&self) -> CommitId {
        self.observed_commit
    }
}

impl CandidateSnapshot {
    /// Returns the owning workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.manifest.workspace_id()
    }
    /// Returns the stable snapshot identity.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.manifest.snapshot_id()
    }
    /// Returns the immutable synthetic commit.
    #[must_use]
    pub const fn commit(&self) -> CommitId {
        self.manifest.commit()
    }
    /// Returns the exact candidate tree.
    #[must_use]
    pub const fn tree(&self) -> TreeId {
        self.manifest.tree()
    }
    /// Returns the namespaced reference retaining the snapshot.
    #[must_use]
    pub const fn reference(&self) -> &SnapshotRef {
        self.manifest.reference()
    }
    /// Returns the canonical snapshot manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest.digest()
    }
    /// Returns the canonical versioned retained-snapshot manifest.
    #[must_use]
    pub const fn manifest(&self) -> &CandidateSnapshotManifest {
        &self.manifest
    }
    /// Returns a baseline suitable for a detached read-only snapshot worktree.
    #[must_use]
    pub const fn baseline(&self) -> Baseline {
        Baseline::checked(self.commit(), self.tree())
    }
}

/// Request to materialize a retained snapshot tree in a writable worktree.
#[derive(Clone, Copy, Debug)]
pub struct RestoreRequest<'a> {
    worktree: &'a RegisteredWorktree,
    snapshot: &'a CandidateSnapshot,
    expected_head: CommitId,
}

impl<'a> RestoreRequest<'a> {
    /// Binds restoration to an exact registration, retained snapshot, and current HEAD.
    #[must_use]
    pub const fn new(
        worktree: &'a RegisteredWorktree,
        snapshot: &'a CandidateSnapshot,
        expected_head: CommitId,
    ) -> Self {
        Self { worktree, snapshot, expected_head }
    }
}

/// Checked before/after evidence from restoring an immutable snapshot tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreObservation {
    prior_tree: Option<TreeId>,
    restored_tree: TreeId,
    status: StatusObservation,
}

impl RestoreObservation {
    /// Returns the index tree before restoration, if conflicts allowed one to be written.
    #[must_use]
    pub const fn prior_tree(&self) -> Option<TreeId> {
        self.prior_tree
    }
    /// Returns the exact restored index tree.
    #[must_use]
    pub const fn restored_tree(&self) -> TreeId {
        self.restored_tree
    }
    /// Returns the post-restore status observation.
    #[must_use]
    pub const fn status(&self) -> &StatusObservation {
        &self.status
    }
}
