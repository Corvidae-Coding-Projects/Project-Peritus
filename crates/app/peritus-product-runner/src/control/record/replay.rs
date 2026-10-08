//! Revision fencing derived from exact history, never from a mutable cached counter.

use super::{ControlError, ControlIntent, ControlOperation, ControlReceipt, ConversationRecord};
use crate::control::{CheckpointId, UserCheckpoint};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const REPLAY_CHECKPOINT_SCHEMA: u16 = 1;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReplayCheckpoint {
    schema: u16,
    current: Option<ConversationRecord>,
    edit_revision: u64,
    automatic_checkpoints: Vec<UserCheckpoint>,
}

/// Replays a conversation while retaining the last revision that changed user-controlled state.
/// Background accounting may advance after an inspected revision without invalidating an edit.
#[derive(Debug, Default)]
pub struct ConversationReplay {
    current: Option<ConversationRecord>,
    edit_revision: u64,
    automatic_checkpoints: BTreeMap<CheckpointId, UserCheckpoint>,
}

impl ConversationReplay {
    /// Borrows the exact latest projection.
    #[must_use]
    pub const fn current(&self) -> Option<&ConversationRecord> {
        self.current.as_ref()
    }

    /// Borrows one automatic checkpoint reconstructed from immutable control history.
    #[must_use]
    pub fn automatic_checkpoint(&self, id: CheckpointId) -> Option<&UserCheckpoint> {
        self.automatic_checkpoints.get(&id)
    }

    /// Encodes the complete effect-free replay context for a derived durable projection.
    ///
    /// # Errors
    /// Rejects a representation that cannot be encoded canonically.
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>, ControlError> {
        let checkpoint = ReplayCheckpoint {
            schema: REPLAY_CHECKPOINT_SCHEMA,
            current: self.current.clone(),
            edit_revision: self.edit_revision,
            automatic_checkpoints: self.automatic_checkpoints.values().cloned().collect(),
        };
        serde_json::to_vec(&checkpoint).map_err(|_| ControlError::Capacity)
    }

    /// Restores replay context from exact canonical derived-projection bytes.
    ///
    /// # Errors
    /// Rejects unsupported, noncanonical, duplicate, or internally inconsistent context.
    pub fn from_checkpoint_bytes(bytes: &[u8]) -> Result<Self, ControlError> {
        let checkpoint: ReplayCheckpoint =
            serde_json::from_slice(bytes).map_err(|_| ControlError::InvalidInput)?;
        let current_revision = checkpoint.current.as_ref().map_or(0, ConversationRecord::revision);
        if checkpoint.schema != REPLAY_CHECKPOINT_SCHEMA
            || checkpoint.edit_revision > current_revision
            || serde_json::to_vec(&checkpoint).map_err(|_| ControlError::InvalidInput)? != bytes
        {
            return Err(ControlError::InvalidInput);
        }
        if let Some(current) = checkpoint.current.as_ref() {
            current.canonical_bytes().map_err(|_| ControlError::InvalidInput)?;
        }
        let ReplayCheckpoint {
            current,
            edit_revision,
            automatic_checkpoints: encoded_automatic,
            ..
        } = checkpoint;
        let mut automatic_checkpoints = BTreeMap::new();
        let mut previous = None;
        for checkpoint in encoded_automatic {
            let id = checkpoint.id();
            if checkpoint.automatic_run().is_none()
                || previous.is_some_and(|previous| previous >= id)
                || automatic_checkpoints.insert(checkpoint.id(), checkpoint).is_some()
            {
                return Err(ControlError::InvalidInput);
            }
            previous = Some(id);
        }
        if current.is_none() && (edit_revision != 0 || !automatic_checkpoints.is_empty())
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { current, edit_revision, automatic_checkpoints })
    }

    /// Applies one operation, preserving its original identity and inspected revision.
    ///
    /// # Errors
    /// Rejects competing edits, invalid lifecycle transitions, and mismatched ownership.
    pub fn apply(&mut self, operation: &ControlOperation) -> Result<ControlReceipt, ControlError> {
        let automatic = match operation.intent() {
            ControlIntent::CreateAutomaticCheckpoint(checkpoint) => {
                if self.automatic_checkpoints.contains_key(&checkpoint.id()) {
                    return Err(ControlError::IdempotencyConflict);
                }
                Some((checkpoint.id(), checkpoint.clone()))
            }
            ControlIntent::SealAutomaticCheckpoint { checkpoint, run, versions } => {
                let mut value = self
                    .automatic_checkpoints
                    .get(checkpoint)
                    .cloned()
                    .ok_or(ControlError::NotFound)?;
                if value.automatic_run() != Some(*run) {
                    return Err(ControlError::IdempotencyConflict);
                }
                value.seal(*run, versions)?;
                Some((*checkpoint, value))
            }
            ControlIntent::ReserveAutomaticFork { branch, checkpoint, .. } => {
                let replayed = self
                    .automatic_checkpoints
                    .get(&checkpoint.id())
                    .ok_or(ControlError::NotFound)?;
                let branch_selects_checkpoint =
                    checkpoint.id().as_bytes() == branch.checkpoint().as_bytes();
                if replayed != checkpoint.as_ref() || !branch_selects_checkpoint {
                    return Err(ControlError::IdempotencyConflict);
                }
                None
            }
            ControlIntent::PrepareAutomaticRestore { checkpoint, .. } => {
                let replayed = self
                    .automatic_checkpoints
                    .get(&checkpoint.id())
                    .ok_or(ControlError::NotFound)?;
                if replayed != checkpoint.as_ref() {
                    return Err(ControlError::IdempotencyConflict);
                }
                None
            }
            ControlIntent::SettleAutomaticRestore { checkpoint, checkpoint_versions, .. } => {
                let replayed =
                    self.automatic_checkpoints.get(checkpoint).ok_or(ControlError::NotFound)?;
                if replayed.paths().len() != checkpoint_versions.len()
                    || replayed.paths().iter().zip(checkpoint_versions).any(
                        |(path, (name, version))| {
                            path.path() != name || path.checkpoint() != *version
                        },
                    )
                {
                    return Err(ControlError::IdempotencyConflict);
                }
                None
            }
            _ => None,
        };
        let (next, receipt) = ConversationRecord::apply_after_accounting(
            self.current.as_ref(),
            operation,
            self.edit_revision,
        )?;
        if let Some((id, checkpoint)) = automatic {
            self.automatic_checkpoints.insert(id, checkpoint);
        }
        if !matches!(
            operation.intent(),
            ControlIntent::ObserveGoalProgress { .. }
                | ControlIntent::ReserveGoalRequest { .. }
                | ControlIntent::CompleteGoalRequest { .. }
                | ControlIntent::ReserveGoalTool { .. }
                | ControlIntent::CompleteGoalTool { .. }
        ) {
            self.edit_revision = next.revision();
        }
        self.current = Some(next);
        Ok(receipt)
    }
}

impl ControlOperation {
    pub(super) const fn can_follow_accounting(&self) -> bool {
        matches!(
            self.intent,
            ControlIntent::Queue(
                crate::control::QueueIntent::Enqueue { .. }
                    | crate::control::QueueIntent::EnqueueSource { .. }
                    | crate::control::QueueIntent::Edit { .. }
                    | crate::control::QueueIntent::EditSource { .. }
                    | crate::control::QueueIntent::Correct { .. }
                    | crate::control::QueueIntent::CorrectSource { .. }
                    | crate::control::QueueIntent::Hold { .. }
                    | crate::control::QueueIntent::Withdraw(_)
                    | crate::control::QueueIntent::Move { .. }
                    | crate::control::QueueIntent::Reorder(_)
            ) | ControlIntent::SetBrief { .. }
                | ControlIntent::ClearGoal { .. }
                | ControlIntent::RenameConversation { .. }
                | ControlIntent::PinConversation { .. }
        )
    }
}
