//! Checkpoint and restore aggregate transitions.

use super::{ControlError, ControlIntent, ControlOperation, ConversationRecord};

impl ConversationRecord {
    pub(super) fn apply_checkpoint(
        &mut self,
        operation: &ControlOperation,
    ) -> Result<(), ControlError> {
        match &operation.intent {
            ControlIntent::CreateCheckpoint(checkpoint) => {
                if checkpoint.id().as_bytes() != operation.id.as_bytes()
                    || self.checkpoints.len() >= crate::control::MAX_CHECKPOINTS
                    || self.checkpoints.iter().any(|prior| prior.id() == checkpoint.id())
                {
                    return Err(ControlError::InvalidInput);
                }
                self.checkpoints.push(checkpoint.clone());
                Ok(())
            }
            ControlIntent::SealCheckpoint { checkpoint, run, versions } => {
                let checkpoint = self
                    .checkpoints
                    .iter_mut()
                    .find(|candidate| candidate.id() == *checkpoint)
                    .ok_or(ControlError::NotFound)?;
                checkpoint.seal(*run, versions)
            }
            ControlIntent::PrepareRestore { restore, recovery } => {
                if restore.id().as_bytes() != operation.id.as_bytes()
                    || restore.status() != crate::control::RestoreStatus::Prepared
                    || restore.recovery_checkpoint() != recovery.id()
                    || !self.checkpoints.iter().any(|value| value.id() == restore.checkpoint())
                    || self.checkpoints.iter().any(|value| value.id() == recovery.id())
                    || self.restores.len() >= crate::control::MAX_RESTORES
                    || self.restores.iter().any(|value| value.id() == restore.id())
                {
                    return Err(ControlError::InvalidInput);
                }
                if let Some(branch) = restore.branch() {
                    let observed = self
                        .goal
                        .as_ref()
                        .map_or(0, crate::control::GoalRecord::updated_unix_millis);
                    self.reserve_fork(branch, observed)?;
                }
                self.checkpoints.push(recovery.clone());
                self.restores.push(restore.clone());
                Ok(())
            }
            ControlIntent::SettleRestore {
                restore,
                status,
                conflicts,
                transaction_manifest_digest,
            } => {
                let restore = self
                    .restores
                    .iter_mut()
                    .find(|candidate| candidate.id() == *restore)
                    .ok_or(ControlError::NotFound)?;
                restore.settle(
                    *status,
                    conflicts.clone(),
                    transaction_manifest_digest.map(peritus_types::Sha256Digest::new),
                )?;
                if matches!(
                    status,
                    crate::control::RestoreStatus::Applied
                        | crate::control::RestoreStatus::RecoveryRequired
                ) {
                    // The restore event is the durable observation. Invalidate evidence without
                    // consuming message capacity after the filesystem effect has already settled.
                    self.inputs = self.inputs.context_changed()?;
                    if let Some(goal) = &mut self.goal {
                        goal.requirements_changed(
                            self.inputs.generation(),
                            false,
                            goal.updated_unix_millis(),
                        )?;
                    }
                }
                Ok(())
            }
            _ => Err(ControlError::InvalidInput),
        }
    }
}
