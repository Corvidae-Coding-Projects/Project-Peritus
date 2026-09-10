//! Durable restore outcome and exact transaction bindings.

use super::{
    CheckpointId, ControlError, ControlText, MAX_CHECKPOINT_PATHS, RestoreId, Sha256Digest,
};
use serde::Deserialize;
use serde::Serialize;

/// Durable outcome state for a preview-bound restore operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreStatus {
    /// Current covered bytes were retained and the filesystem transaction is not yet settled.
    Prepared,
    /// Every covered target was restored and verified.
    Applied,
    /// A covered path differed from the completed owned version; no bytes were overwritten.
    Conflict,
    /// The filesystem transaction cannot prove one consistent pre- or post-restore state.
    RecoveryRequired,
}

/// Durable restore operation. It never contains or modifies accounting and authority state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreOperation {
    id: RestoreId,
    checkpoint: CheckpointId,
    preview_digest: [u8; 32],
    patch_digest: [u8; 32],
    recovery_checkpoint: CheckpointId,
    status: RestoreStatus,
    conflicts: Vec<ControlText<4096>>,
    transaction_manifest_digest: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<Box<crate::control::ConversationBranch>>,
}
impl RestoreOperation {
    /// Constructs a prepared exact restore journal record.
    ///
    /// # Errors
    /// Rejects invalid identity or state bindings.
    pub const fn prepared(
        id: RestoreId,
        checkpoint: CheckpointId,
        preview_digest: Sha256Digest,
        patch_digest: Sha256Digest,
        recovery_checkpoint: CheckpointId,
    ) -> Result<Self, ControlError> {
        Ok(Self {
            id,
            checkpoint,
            preview_digest: preview_digest.into_bytes(),
            patch_digest: patch_digest.into_bytes(),
            recovery_checkpoint,
            status: RestoreStatus::Prepared,
            conflicts: Vec::new(),
            transaction_manifest_digest: None,
            branch: None,
        })
    }
    /// Retains the exact logical branch to publish only after successful file settlement.
    ///
    /// # Errors
    /// Rejects a branch for another checkpoint or a terminal restore.
    pub fn with_branch(
        mut self,
        branch: crate::control::ConversationBranch,
    ) -> Result<Self, ControlError> {
        if branch.checkpoint().as_bytes() != self.checkpoint.as_bytes()
            || self.status != RestoreStatus::Prepared
        {
            return Err(ControlError::InvalidInput);
        }
        self.branch = Some(Box::new(branch));
        Ok(self)
    }
    /// Borrows the reserved non-running logical branch, if requested.
    #[must_use]
    pub fn branch(&self) -> Option<&crate::control::ConversationBranch> {
        self.branch.as_deref()
    }
    /// Returns restore identity.
    #[must_use]
    pub const fn id(&self) -> RestoreId {
        self.id
    }
    /// Returns selected checkpoint identity.
    #[must_use]
    pub const fn checkpoint(&self) -> CheckpointId {
        self.checkpoint
    }
    /// Returns the exact preview fingerprint confirmed by the user.
    #[must_use]
    pub const fn preview_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.preview_digest)
    }
    /// Returns the exact checked patch transaction identity.
    #[must_use]
    pub const fn patch_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.patch_digest)
    }
    /// Returns the pre-apply recovery checkpoint retained in the journal.
    #[must_use]
    pub const fn recovery_checkpoint(&self) -> CheckpointId {
        self.recovery_checkpoint
    }
    /// Returns truthful current settlement state.
    #[must_use]
    pub const fn status(&self) -> RestoreStatus {
        self.status
    }
    /// Returns exact terminal transaction evidence, when one was durably retained.
    #[must_use]
    pub const fn transaction_manifest_digest(&self) -> Option<Sha256Digest> {
        match self.transaction_manifest_digest {
            Some(value) => Some(Sha256Digest::new(value)),
            None => None,
        }
    }
    /// Borrows exact paths that blocked the restore.
    pub fn conflicts(&self) -> crate::control::ControlTextIter<'_, 4096> {
        self.conflicts.iter().map(ControlText::as_str)
    }
    pub(in crate::control) fn settle(
        &mut self,
        status: RestoreStatus,
        conflicts: Vec<String>,
        transaction_manifest_digest: Option<Sha256Digest>,
    ) -> Result<(), ControlError> {
        if self.status != RestoreStatus::Prepared || status == RestoreStatus::Prepared {
            return Err(ControlError::InvalidInput);
        }
        if conflicts.len() > MAX_CHECKPOINT_PATHS
            || (status == RestoreStatus::Conflict) == conflicts.is_empty()
            || status == RestoreStatus::Applied && transaction_manifest_digest.is_none()
        {
            return Err(ControlError::InvalidInput);
        }
        self.conflicts = conflicts.into_iter().map(ControlText::new).collect::<Result<_, _>>()?;
        self.status = status;
        self.transaction_manifest_digest =
            transaction_manifest_digest.map(Sha256Digest::into_bytes);
        Ok(())
    }
}
