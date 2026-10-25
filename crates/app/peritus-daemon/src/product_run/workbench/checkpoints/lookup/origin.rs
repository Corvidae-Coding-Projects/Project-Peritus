//! Read the exact operation that durably created a checkpoint, including restore recovery.

use super::super::{
    ControlError, ControlIntent, ControlStore, ConversationRecord, Error, OperationId,
    RestoreOperation, UserCheckpoint,
};

pub(in crate::product_run::workbench::checkpoints) fn checkpoint_revision(
    store: &ControlStore,
    record: &ConversationRecord,
    checkpoint: &UserCheckpoint,
) -> Result<u64, Error> {
    let mut recoveries =
        record.restores().iter().filter(|restore| restore.recovery_checkpoint() == checkpoint.id());
    let operation = if let Some(restore) = recoveries.next() {
        if recoveries.next().is_some() {
            return Err(Error::Corrupt("checkpoint has multiple recovery origins"));
        }
        // Recovery has its own checkpoint identity, while the accepted journal
        // operation is the restore preparation that atomically retained its bytes.
        let operation = store
            .operation(record.id(), OperationId::new(*restore.id().as_bytes())?)?
            .ok_or(Error::Corrupt("recovery checkpoint preparation missing"))?;
        let (prepared, recovery) = match operation.intent() {
            ControlIntent::PrepareRestore { restore, recovery }
            | ControlIntent::PrepareAutomaticRestore { restore, recovery, .. } => {
                (restore, recovery)
            }
            _ => return Err(Error::Corrupt("recovery checkpoint origin has another intent")),
        };
        let mut expected = RestoreOperation::prepared(
            restore.id(),
            restore.checkpoint(),
            restore.preview_digest(),
            restore.patch_digest(),
            restore.recovery_checkpoint(),
        )?;
        if let Some(branch) = restore.branch() {
            expected = expected.with_branch(branch.clone())?;
        }
        if prepared != &expected || recovery != &checkpoint.capture_manifest() {
            return Err(Error::Corrupt("recovery checkpoint differs from its preparation"));
        }
        operation
    } else {
        let operation = store
            .operation(record.id(), OperationId::new(*checkpoint.id().as_bytes())?)?
            .ok_or(ControlError::NotFound)?;
        let captured = match operation.intent() {
            ControlIntent::CreateCheckpoint(captured)
            | ControlIntent::CreateAutomaticCheckpoint(captured) => captured,
            _ => return Err(Error::Corrupt("checkpoint origin has another intent")),
        };
        if captured != &checkpoint.capture_manifest() {
            return Err(Error::Corrupt("checkpoint differs from its capture operation"));
        }
        operation
    };
    if operation.actor_bytes() != record.owner_bytes()
        || operation.workspace_bytes() != record.workspace_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    // Resolve the original immutable operation, rather than minting a checkpoint
    // operation or substituting the current/settlement revision for capture time.
    let receipt =
        store.resolve(&operation)?.ok_or(Error::Corrupt("checkpoint origin receipt missing"))?;
    Ok(receipt.accepted_revision())
}
