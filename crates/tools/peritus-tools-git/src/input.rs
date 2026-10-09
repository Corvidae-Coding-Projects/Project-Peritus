//! Checked bounded Git-tool inputs.

use peritus_types::SnapshotId;

use crate::{GitToolError, GitToolOperation};

/// Status requires no caller-selected Git arguments.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatusInput;

/// Bounded immutable commit-diff request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffInput {
    pub(crate) base_revision: String,
    pub(crate) maximum_entries: u32,
    pub(crate) maximum_patch_bytes: u64,
}

impl DiffInput {
    /// Creates an immutable structured diff input.
    ///
    /// # Errors
    /// Rejects option-like/control-bearing revisions and invalid output bounds.
    pub fn new(
        base_revision: String,
        maximum_entries: u32,
        maximum_patch_bytes: u64,
    ) -> Result<Self, GitToolError> {
        if !valid_revision(&base_revision)
            || !crate::verified::diff_bounds_valid(
                maximum_entries,
                maximum_patch_bytes,
                peritus_git::MAX_DIFF_ENTRIES,
                peritus_git::MAX_DIFF_BYTES,
            )
        {
            return Err(GitToolError::invalid(
                GitToolOperation::Diff,
                "revision or diff bounds are invalid",
            ));
        }
        Ok(Self { base_revision, maximum_entries, maximum_patch_bytes })
    }
}

/// Bounded immutable history input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryInput {
    pub(crate) maximum_commits: u16,
}

impl HistoryInput {
    /// Creates a bounded history input.
    ///
    /// # Errors
    /// Rejects zero or excessive commit counts.
    pub const fn new(maximum_commits: u16) -> Result<Self, GitToolError> {
        if maximum_commits == 0 || maximum_commits > peritus_git::MAX_HISTORY_COMMITS {
            return Err(GitToolError::invalid(
                GitToolOperation::History,
                "history count is outside its bound",
            ));
        }
        Ok(Self { maximum_commits })
    }
}

/// Snapshot inspection selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotInput {
    /// Inspect the immutable C1 workspace snapshot.
    Current,
    /// Inspect a caller-resolved retained C1 candidate snapshot with this exact identity.
    Retained(SnapshotId),
}

/// Authorized candidate-plus-snapshot input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateInput {
    snapshot_id: SnapshotId,
}

impl CandidateInput {
    /// Creates a stable successor snapshot request.
    #[must_use]
    pub const fn new(snapshot_id: SnapshotId) -> Self {
        Self { snapshot_id }
    }
    /// Returns the exact new snapshot identity.
    #[must_use]
    pub const fn snapshot_id(self) -> SnapshotId {
        self.snapshot_id
    }
}

/// Authorized history-preserving rollback input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RollbackInput {
    target_snapshot_id: SnapshotId,
    successor_snapshot_id: SnapshotId,
}

impl RollbackInput {
    /// Creates an exact target and distinct successor snapshot request.
    ///
    /// # Errors
    /// Rejects identity reuse because rollback must retain history as a successor.
    pub fn new(
        target_snapshot_id: SnapshotId,
        successor_snapshot_id: SnapshotId,
    ) -> Result<Self, GitToolError> {
        if target_snapshot_id == successor_snapshot_id {
            return Err(GitToolError::invalid(
                GitToolOperation::Rollback,
                "rollback target and successor snapshot identities must differ",
            ));
        }
        Ok(Self { target_snapshot_id, successor_snapshot_id })
    }
    /// Returns the exact retained target identity.
    #[must_use]
    pub const fn target_snapshot_id(self) -> SnapshotId {
        self.target_snapshot_id
    }
    /// Returns the exact successor identity.
    #[must_use]
    pub const fn successor_snapshot_id(self) -> SnapshotId {
        self.successor_snapshot_id
    }
}

fn valid_revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1_024
        && !value.starts_with('-')
        && !value.bytes().any(|byte| byte == 0 || byte.is_ascii_control())
}
