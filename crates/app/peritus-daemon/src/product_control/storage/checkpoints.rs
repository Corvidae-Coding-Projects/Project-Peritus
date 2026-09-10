//! Checkpoint before-images and restore transaction evidence in the control journal.

use super::{ControlError, ControlOperation, ControlReceipt, ControlStore, Error};
use peritus_journal::StateInstall;
use peritus_product_runner::control::{
    CheckpointFileVersion, CheckpointId, ControlIntent, RestoreStatus, UserCheckpoint,
};

const CHECKPOINT_BODY_NAMESPACE: u16 = 3480;
const RESTORE_TRANSACTION_NAMESPACE: u16 = 3481;

impl ControlStore {
    /// Atomically publishes a checkpoint manifest and all present before-images.
    pub(crate) fn accept_checkpoint(
        &mut self,
        operation: &ControlOperation,
        bodies: &[Option<Vec<u8>>],
    ) -> Result<ControlReceipt, Error> {
        let ControlIntent::CreateCheckpoint(checkpoint) = operation.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        let installs = checkpoint_installs(checkpoint, bodies)?;
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        self.accept_installs(operation, installs)
    }

    /// Atomically records an exact recovery checkpoint before any restore filesystem effect.
    pub(crate) fn accept_restore_preparation(
        &mut self,
        operation: &ControlOperation,
        bodies: &[Option<Vec<u8>>],
    ) -> Result<ControlReceipt, Error> {
        let ControlIntent::PrepareRestore { recovery, .. } = operation.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        let installs = checkpoint_installs(recovery, bodies)?;
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        self.accept_installs(operation, installs)
    }

    /// Publishes terminal restore state and exact digest-bound transaction evidence.
    pub(crate) fn accept_restore_settlement(
        &mut self,
        operation: &ControlOperation,
        transaction_manifest: Option<Vec<u8>>,
    ) -> Result<ControlReceipt, Error> {
        let ControlIntent::SettleRestore { restore, status, transaction_manifest_digest, .. } =
            operation.intent()
        else {
            return Err(ControlError::InvalidInput.into());
        };
        let installs = match (status, transaction_manifest_digest, transaction_manifest) {
            (
                RestoreStatus::Applied | RestoreStatus::Conflict | RestoreStatus::RecoveryRequired,
                Some(expected),
                Some(bytes),
            ) if peritus_codec::sha256(&bytes).as_bytes() == expected => {
                vec![StateInstall::new(
                    RESTORE_TRANSACTION_NAMESPACE,
                    restore.as_bytes().to_vec(),
                    None,
                    1,
                    bytes,
                )?]
            }
            (RestoreStatus::Conflict | RestoreStatus::RecoveryRequired, None, None) => Vec::new(),
            _ => return Err(ControlError::InvalidInput.into()),
        };
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        self.accept_installs(operation, installs)
    }

    /// Reads one exact retained checkpoint body after immutable replay verification.
    pub(crate) fn checkpoint_body(
        &self,
        checkpoint: CheckpointId,
        index: usize,
        version: CheckpointFileVersion,
    ) -> Result<Option<Vec<u8>>, Error> {
        let Some(expected) = version.digest() else { return Ok(None) };
        let record = self
            .journal
            .state_record(CHECKPOINT_BODY_NAMESPACE, &checkpoint_key(checkpoint, index)?)?
            .ok_or(Error::Corrupt("checkpoint before-image missing"))?;
        let bytes = record.bytes();
        if version.bytes() != Some(bytes.len() as u64) || peritus_codec::sha256(bytes) != expected {
            return Err(Error::Corrupt("checkpoint before-image differs from its manifest"));
        }
        Ok(Some(bytes.to_vec()))
    }

    pub(super) fn verify_checkpoint_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
    ) -> Result<(), Error> {
        match operation.intent() {
            ControlIntent::CreateCheckpoint(checkpoint) => {
                self.verify_checkpoint_bodies(checkpoint, position)
            }
            ControlIntent::PrepareRestore { recovery, .. } => {
                self.verify_checkpoint_bodies(recovery, position)
            }
            ControlIntent::SettleRestore {
                restore,
                status:
                    RestoreStatus::Applied | RestoreStatus::Conflict | RestoreStatus::RecoveryRequired,
                transaction_manifest_digest: Some(expected),
                ..
            } => {
                let record = self
                    .journal
                    .state_record(RESTORE_TRANSACTION_NAMESPACE, restore.as_bytes())?
                    .ok_or(Error::Corrupt("applied restore transaction receipt missing"))?;
                if record.revision() != 1
                    || record.producing_position() != position
                    || peritus_codec::sha256(record.bytes()).as_bytes() != expected
                {
                    return Err(Error::Corrupt(
                        "restore transaction evidence differs from its journal event",
                    ));
                }
                Ok(())
            }
            ControlIntent::SettleRestore {
                status: RestoreStatus::Conflict | RestoreStatus::RecoveryRequired,
                transaction_manifest_digest: None,
                ..
            } => Ok(()),
            _ => Err(Error::Corrupt("invalid checkpoint artifact operation")),
        }
    }

    fn verify_checkpoint_bodies(
        &self,
        checkpoint: &UserCheckpoint,
        position: u64,
    ) -> Result<(), Error> {
        for (index, path) in checkpoint.paths().iter().enumerate() {
            match path.checkpoint() {
                CheckpointFileVersion::Absent => {
                    if self
                        .journal
                        .state_record(
                            CHECKPOINT_BODY_NAMESPACE,
                            &checkpoint_key(checkpoint.id(), index)?,
                        )?
                        .is_some()
                    {
                        return Err(Error::Corrupt("absent checkpoint path has retained bytes"));
                    }
                }
                version @ CheckpointFileVersion::Present { .. } => {
                    let record = self
                        .journal
                        .state_record(
                            CHECKPOINT_BODY_NAMESPACE,
                            &checkpoint_key(checkpoint.id(), index)?,
                        )?
                        .ok_or(Error::Corrupt("checkpoint before-image missing"))?;
                    if record.revision() != 1 || record.producing_position() != position {
                        return Err(Error::Corrupt(
                            "checkpoint before-image was not published with its manifest",
                        ));
                    }
                    let expected = version
                        .digest()
                        .ok_or(Error::Corrupt("present checkpoint path has no digest"))?;
                    if version.bytes() != Some(record.bytes().len() as u64)
                        || peritus_codec::sha256(record.bytes()) != expected
                    {
                        return Err(Error::Corrupt(
                            "checkpoint before-image differs from its manifest",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn checkpoint_installs(
    checkpoint: &UserCheckpoint,
    bodies: &[Option<Vec<u8>>],
) -> Result<Vec<StateInstall>, Error> {
    if checkpoint.paths().len() != bodies.len() {
        return Err(ControlError::InvalidInput.into());
    }
    let mut installs = Vec::new();
    for (index, (path, body)) in checkpoint.paths().iter().zip(bodies).enumerate() {
        match (path.checkpoint(), body) {
            (CheckpointFileVersion::Absent, None) => {}
            (version, Some(bytes))
                if version.bytes() == Some(bytes.len() as u64)
                    && version.digest() == Some(peritus_codec::sha256(bytes)) =>
            {
                installs.push(StateInstall::new(
                    CHECKPOINT_BODY_NAMESPACE,
                    checkpoint_key(checkpoint.id(), index)?,
                    None,
                    1,
                    bytes.clone(),
                )?);
            }
            _ => return Err(ControlError::InvalidInput.into()),
        }
    }
    Ok(installs)
}

fn checkpoint_key(checkpoint: CheckpointId, index: usize) -> Result<Vec<u8>, Error> {
    let index = u16::try_from(index).map_err(|_| ControlError::Capacity)?;
    let mut key = checkpoint.as_bytes().to_vec();
    key.extend_from_slice(&index.to_be_bytes());
    Ok(key)
}
