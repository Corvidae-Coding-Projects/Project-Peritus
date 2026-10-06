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
                    || self.checkpoints.iter().any(|prior| prior.id() == checkpoint.id())
                {
                    return Err(ControlError::InvalidInput);
                }
                self.checkpoints.push(checkpoint.clone());
                Ok(())
            }
            ControlIntent::CreateAutomaticCheckpoint(checkpoint) => {
                if checkpoint.id().as_bytes() != operation.id.as_bytes()
                    || checkpoint.automatic_run().is_none()
                    || checkpoint.capture_manifest() != *checkpoint
                {
                    return Err(ControlError::InvalidInput);
                }
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
            ControlIntent::SealAutomaticCheckpoint { run, .. } => {
                if *run == [0; 16] {
                    return Err(ControlError::InvalidInput);
                }
                Ok(())
            }
            ControlIntent::PrepareRestore { restore, recovery } => {
                self.prepare_restore(operation, restore, None, recovery)
            }
            ControlIntent::PrepareAutomaticRestore { restore, checkpoint, recovery } => {
                self.prepare_restore(operation, restore, Some(checkpoint.as_ref()), recovery)
            }
            ControlIntent::SettleRestore {
                restore,
                status,
                conflicts,
                transaction_manifest_digest,
                seal_recovery,
            } => self.settle_restore(
                *restore,
                None,
                *status,
                conflicts,
                *transaction_manifest_digest,
                *seal_recovery,
            ),
            ControlIntent::SettleAutomaticRestore {
                restore,
                checkpoint,
                status,
                conflicts,
                transaction_manifest_digest,
                seal_recovery,
                checkpoint_versions,
            } => self.settle_restore(
                *restore,
                Some((*checkpoint, checkpoint_versions.as_slice())),
                *status,
                conflicts,
                *transaction_manifest_digest,
                *seal_recovery,
            ),
            _ => Err(ControlError::InvalidInput),
        }
    }

    fn prepare_restore(
        &mut self,
        operation: &ControlOperation,
        restore: &crate::control::RestoreOperation,
        replayed: Option<&crate::control::UserCheckpoint>,
        recovery: &crate::control::UserCheckpoint,
    ) -> Result<(), ControlError> {
        let retained = self.checkpoints.iter().find(|value| value.id() == restore.checkpoint());
        let source_exact = match (retained, replayed) {
            (Some(_), None) => true,
            (None, Some(checkpoint)) => {
                checkpoint.id() == restore.checkpoint() && checkpoint.automatic_run().is_some()
            }
            _ => false,
        };
        if restore.id().as_bytes() != operation.id.as_bytes()
            || restore.status() != crate::control::RestoreStatus::Prepared
            || restore.recovery_checkpoint() != recovery.id()
            || !source_exact
            || self.checkpoints.iter().any(|value| value.id() == recovery.id())
            || self.restores.iter().any(|value| value.id() == restore.id())
        {
            return Err(ControlError::InvalidInput);
        }
        if let Some(branch) = restore.branch() {
            self.validate_fork(branch, replayed)?;
        }
        restore.validate_targets(retained.or(replayed).ok_or(ControlError::NotFound)?, recovery)?;
        self.checkpoints.push(recovery.clone());
        self.restores.push(restore.clone());
        Ok(())
    }

    fn settle_restore(
        &mut self,
        restore_id: crate::control::RestoreId,
        replayed: Option<(
            crate::control::CheckpointId,
            &[(String, crate::control::CheckpointFileVersion)],
        )>,
        status: crate::control::RestoreStatus,
        conflicts: &[String],
        transaction_manifest_digest: Option<[u8; 32]>,
        seal_recovery: bool,
    ) -> Result<(), ControlError> {
        let restore = self
            .restores
            .iter_mut()
            .find(|candidate| candidate.id() == restore_id)
            .ok_or(ControlError::NotFound)?;
        if replayed.is_some_and(|(checkpoint, _)| checkpoint != restore.checkpoint()) {
            return Err(ControlError::InvalidInput);
        }
        restore.settle(
            status,
            conflicts.to_vec(),
            transaction_manifest_digest.map(peritus_types::Sha256Digest::new),
        )?;
        if seal_recovery && status == crate::control::RestoreStatus::Applied {
            let recovery_id = restore.recovery_checkpoint();
            let retained_versions;
            let versions = if let Some(targets) = restore.targets() {
                retained_versions = targets
                    .iter()
                    .map(|target| (target.path().to_owned(), target.checkpoint()))
                    .collect::<Vec<_>>();
                if replayed.is_some_and(|(_, versions)| versions != retained_versions.as_slice()) {
                    return Err(ControlError::InvalidInput);
                }
                &retained_versions
            } else if let Some((_, versions)) = replayed {
                versions
            } else {
                let source = self
                    .checkpoints
                    .iter()
                    .find(|value| value.id() == restore.checkpoint())
                    .ok_or(ControlError::NotFound)?;
                retained_versions = source
                    .paths()
                    .iter()
                    .map(|path| (path.path().to_owned(), path.checkpoint()))
                    .collect::<Vec<_>>();
                &retained_versions
            };
            let recovery = self
                .checkpoints
                .iter_mut()
                .find(|value| value.id() == recovery_id)
                .ok_or(ControlError::NotFound)?;
            // Conversation-only rewind has no filesystem effects or coverage.
            if !recovery.paths().is_empty() {
                recovery.seal_restoration(versions)?;
            }
        }
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
}
