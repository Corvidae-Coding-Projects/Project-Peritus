//! Durable restore outcomes and their compact paged-protocol form.

use super::invalid;
use crate::{AppProtocolError, ControlOperationId, WorkbenchQuery};
use peritus_types::Sha256Digest;

/// Public terminal restore state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchRestoreStatus {
    /// All planned covered paths were restored and verified.
    Applied,
    /// At least one covered path conflicts; no workspace byte was changed.
    Conflict,
    /// A prior interrupted operation requires explicit recovery inspection.
    RecoveryRequired,
}

/// Durable post-confirmation receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchRestoreReceipt {
    restore: ControlOperationId,
    checkpoint: ControlOperationId,
    recovery_checkpoint: ControlOperationId,
    query: WorkbenchQuery,
    accepted_revision: u64,
    status: WorkbenchRestoreStatus,
    restored: Vec<String>,
    conflicts: Vec<String>,
    external_effects: Vec<String>,
}
impl WorkbenchRestoreReceipt {
    /// Constructs one bounded truthful receipt.
    ///
    /// # Errors
    /// Rejects absent revisions, invalid target lists, or inconsistent status.
    #[allow(
        clippy::too_many_arguments,
        reason = "restore receipts retain independent identities and exact outcomes"
    )]
    pub fn new(
        restore: ControlOperationId,
        checkpoint: ControlOperationId,
        recovery_checkpoint: ControlOperationId,
        query: WorkbenchQuery,
        accepted_revision: u64,
        status: WorkbenchRestoreStatus,
        restored: Vec<String>,
        conflicts: Vec<String>,
        external_effects: Vec<String>,
    ) -> Result<Self, AppProtocolError> {
        if accepted_revision == 0
            || u16::try_from(restored.len()).is_err()
            || u16::try_from(conflicts.len()).is_err()
            || u16::try_from(external_effects.len()).is_err()
            || matches!(status, WorkbenchRestoreStatus::Conflict) == conflicts.is_empty()
            || restored.iter().chain(&conflicts).chain(&external_effects).any(|text| {
                text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control)
            })
        {
            return Err(invalid());
        }
        Ok(Self {
            restore,
            checkpoint,
            recovery_checkpoint,
            query,
            accepted_revision,
            status,
            restored,
            conflicts,
            external_effects,
        })
    }
    /// Returns restore operation identity.
    #[must_use]
    pub const fn restore(&self) -> ControlOperationId {
        self.restore
    }
    /// Returns source checkpoint identity.
    #[must_use]
    pub const fn checkpoint(&self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns exact retained pre-apply recovery checkpoint identity.
    #[must_use]
    pub const fn recovery_checkpoint(&self) -> ControlOperationId {
        self.recovery_checkpoint
    }
    /// Returns conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns terminal journal revision.
    #[must_use]
    pub const fn accepted_revision(&self) -> u64 {
        self.accepted_revision
    }
    /// Returns terminal restore status.
    #[must_use]
    pub const fn status(&self) -> WorkbenchRestoreStatus {
        self.status
    }
    /// Borrows paths actually restored.
    #[must_use]
    pub fn restored(&self) -> &[String] {
        &self.restored
    }
    /// Borrows conflicting paths retained untouched.
    #[must_use]
    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }
    /// Borrows external effects explicitly not restored.
    #[must_use]
    pub fn external_effects(&self) -> &[String] {
        &self.external_effects
    }
}

/// Bounded terminal restore outcome for a paged, compactly confirmed checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchRestoreSummary {
    restore: ControlOperationId,
    checkpoint: ControlOperationId,
    recovery_checkpoint: ControlOperationId,
    query: WorkbenchQuery,
    accepted_revision: u64,
    status: WorkbenchRestoreStatus,
    restored_paths: u64,
    conflicting_paths: u64,
    fingerprint: Sha256Digest,
}
impl WorkbenchRestoreSummary {
    /// Constructs a count-only result without imposing a whole-checkpoint list bound.
    ///
    /// # Errors
    /// Rejects invalid identities, revisions, or restore status facts.
    #[allow(clippy::too_many_arguments, reason = "restore summaries retain durable identities")]
    pub fn new(
        restore: ControlOperationId,
        checkpoint: ControlOperationId,
        recovery_checkpoint: ControlOperationId,
        query: WorkbenchQuery,
        accepted_revision: u64,
        status: WorkbenchRestoreStatus,
        restored_paths: u64,
        conflicting_paths: u64,
        fingerprint: Sha256Digest,
    ) -> Result<Self, AppProtocolError> {
        if accepted_revision == 0
            || (status == WorkbenchRestoreStatus::Conflict) != (conflicting_paths != 0)
            || status != WorkbenchRestoreStatus::Applied && restored_paths != 0
        {
            return Err(invalid());
        }
        Ok(Self {
            restore,
            checkpoint,
            recovery_checkpoint,
            query,
            accepted_revision,
            status,
            restored_paths,
            conflicting_paths,
            fingerprint,
        })
    }
    /// Returns restore operation identity.
    #[must_use]
    pub const fn restore(&self) -> ControlOperationId {
        self.restore
    }
    /// Returns source checkpoint identity.
    #[must_use]
    pub const fn checkpoint(&self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns exact retained pre-apply recovery checkpoint identity.
    #[must_use]
    pub const fn recovery_checkpoint(&self) -> ControlOperationId {
        self.recovery_checkpoint
    }
    /// Returns conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns terminal journal revision.
    #[must_use]
    pub const fn accepted_revision(&self) -> u64 {
        self.accepted_revision
    }
    /// Returns terminal restore status.
    #[must_use]
    pub const fn status(&self) -> WorkbenchRestoreStatus {
        self.status
    }
    /// Returns the number of paths actually restored.
    #[must_use]
    pub const fn restored_paths(&self) -> u64 {
        self.restored_paths
    }
    /// Returns the number of conflicting paths retained untouched.
    #[must_use]
    pub const fn conflicting_paths(&self) -> u64 {
        self.conflicting_paths
    }
    /// Returns the full checkpoint and request fingerprint used by paged rewind.
    #[must_use]
    pub const fn fingerprint(&self) -> Sha256Digest {
        self.fingerprint
    }
}
