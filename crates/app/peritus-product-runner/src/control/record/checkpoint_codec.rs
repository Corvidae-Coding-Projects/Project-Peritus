//! Checkpoint metadata has its own paged representation, separate from the bounded control core.

use super::{ControlError, ControlIntent, ControlOperation, ConversationRecord, encode};
use crate::control::{CheckpointId, ConversationBranch, RestoreId, RestoreStatus};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckpointIntentCore<'a> {
    CreateCheckpoint,
    CreateAutomaticCheckpoint,
    SealCheckpoint {
        checkpoint: CheckpointId,
        run: [u8; 16],
    },
    SealAutomaticCheckpoint {
        checkpoint: CheckpointId,
        run: [u8; 16],
    },
    PrepareRestore {
        branch: Option<&'a ConversationBranch>,
    },
    PrepareAutomaticRestore {
        branch: Option<&'a ConversationBranch>,
    },
    SettleRestore {
        restore: RestoreId,
        status: RestoreStatus,
        transaction_manifest_digest: Option<[u8; 32]>,
        seal_recovery: bool,
    },
    SettleAutomaticRestore {
        restore: RestoreId,
        checkpoint: CheckpointId,
        status: RestoreStatus,
        transaction_manifest_digest: Option<[u8; 32]>,
        seal_recovery: bool,
    },
    ReserveAutomaticFork {
        branch: &'a ConversationBranch,
        now_unix_millis: u64,
    },
}

#[derive(Serialize)]
struct CheckpointOperationCore<'a> {
    schema: u16,
    id: crate::control::OperationId,
    conversation: crate::control::ConversationId,
    actor: [u8; 16],
    workspace: [u8; 16],
    expected_revision: u64,
    intent: CheckpointIntentCore<'a>,
}

pub(super) fn encode_operation(operation: &ControlOperation) -> Result<Vec<u8>, ControlError> {
    let intent = match &operation.intent {
        ControlIntent::CreateCheckpoint(_) => CheckpointIntentCore::CreateCheckpoint,
        ControlIntent::CreateAutomaticCheckpoint(_) => {
            CheckpointIntentCore::CreateAutomaticCheckpoint
        }
        ControlIntent::SealCheckpoint { checkpoint, run, .. } => {
            CheckpointIntentCore::SealCheckpoint { checkpoint: *checkpoint, run: *run }
        }
        ControlIntent::SealAutomaticCheckpoint { checkpoint, run, .. } => {
            CheckpointIntentCore::SealAutomaticCheckpoint { checkpoint: *checkpoint, run: *run }
        }
        ControlIntent::PrepareRestore { restore, .. } => {
            CheckpointIntentCore::PrepareRestore { branch: restore.branch() }
        }
        ControlIntent::PrepareAutomaticRestore { restore, .. } => {
            CheckpointIntentCore::PrepareAutomaticRestore { branch: restore.branch() }
        }
        ControlIntent::SettleRestore {
            restore,
            status,
            transaction_manifest_digest,
            seal_recovery,
            ..
        } => CheckpointIntentCore::SettleRestore {
            restore: *restore,
            status: *status,
            transaction_manifest_digest: *transaction_manifest_digest,
            seal_recovery: *seal_recovery,
        },
        ControlIntent::SettleAutomaticRestore {
            restore,
            checkpoint,
            status,
            transaction_manifest_digest,
            seal_recovery,
            ..
        } => CheckpointIntentCore::SettleAutomaticRestore {
            restore: *restore,
            checkpoint: *checkpoint,
            status: *status,
            transaction_manifest_digest: *transaction_manifest_digest,
            seal_recovery: *seal_recovery,
        },
        ControlIntent::ReserveAutomaticFork { branch, now_unix_millis, .. } => {
            CheckpointIntentCore::ReserveAutomaticFork { branch, now_unix_millis: *now_unix_millis }
        }
        ControlIntent::StartGoal { .. } => {
            return serde_json::to_vec(operation).map_err(|_| ControlError::InvalidInput);
        }
        _ => return encode(operation),
    };
    encode(&CheckpointOperationCore {
        schema: operation.schema,
        id: operation.id,
        conversation: operation.conversation,
        actor: operation.actor,
        workspace: operation.workspace,
        expected_revision: operation.expected_revision,
        intent,
    })?;
    // The historical canonical field order and bytes remain the receipt identity.
    serde_json::to_vec(operation).map_err(|_| ControlError::InvalidInput)
}

impl ConversationRecord {
    /// Encodes the control core independently of the checkpoint and restore manifests.
    ///
    /// # Errors
    /// Rejects invalid metadata or a noncheckpoint core exceeding its existing admission bound.
    pub fn canonical_core_bytes(&self) -> Result<Vec<u8>, ControlError> {
        self.validate()?;
        let core = Self {
            schema: self.schema,
            minimum_reader: self.minimum_reader,
            id: self.id,
            owner: self.owner,
            workspace: self.workspace,
            revision: self.revision,
            title: self.title.clone(),
            pinned: self.pinned,
            archived: self.archived,
            inputs: self.inputs.clone(),
            brief: self.brief.clone(),
            context: self.context.clone(),
            prompt_view: self.prompt_view.clone(),
            images: self.images.clone(),
            files: self.files.clone(),
            reviews: self.reviews.clone(),
            execution: self.execution.clone(),
            goal: self.goal.clone(),
            replies: self.replies.clone(),
            checkpoints: Vec::new(),
            restores: Vec::new(),
        };
        if core.goal.is_some() {
            serde_json::to_vec(&core).map_err(|_| ControlError::InvalidInput)
        } else {
            encode(&core)
        }
    }

    /// Encodes the complete checkpoint and restore projection for immutable page publication.
    ///
    /// # Errors
    /// Rejects invalid metadata or an encoding failure.
    pub fn checkpoint_projection_bytes(&self) -> Result<Vec<u8>, ControlError> {
        self.validate()?;
        serde_json::to_vec(&(&self.checkpoints, &self.restores))
            .map_err(|_| ControlError::InvalidInput)
    }

    /// Reconstructs a projection from its independently authenticated core and checkpoint pages.
    ///
    /// # Errors
    /// Rejects malformed, overlapping, noncanonical or invalid components.
    pub fn parse_checkpoint_parts(core: &[u8], checkpoints: &[u8]) -> Result<Self, ControlError> {
        let mut record = Self::parse(core)?;
        if !record.checkpoints.is_empty()
            || !record.restores.is_empty()
            || record.canonical_core_bytes()? != core
        {
            return Err(ControlError::InvalidInput);
        }
        (record.checkpoints, record.restores) =
            serde_json::from_slice(checkpoints).map_err(|_| ControlError::InvalidInput)?;
        if record.checkpoint_projection_bytes()? != checkpoints {
            return Err(ControlError::InvalidInput);
        }
        record.validate()?;
        Ok(record)
    }
}
