//! Staged-file and durable manifest storage inside one transaction directory.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use crate::{PatchError, PatchOperationContext, PatchPlan, RollbackStatus};

use super::{
    FaultInjector, TransactionFaultPoint,
    filesystem::{set_mode, sync_directory},
    manifest::Manifest,
};

pub(super) const MANIFEST_FILE: &str = "manifest.bin";
const NEXT_MANIFEST_FILE: &str = "manifest.next";

pub(super) fn prepare_transaction(
    transaction_directory: &Path,
    plan: &PatchPlan,
    manifest: &Manifest,
    faults: &dyn FaultInjector,
) -> Result<(), PatchError> {
    for (index, operation) in plan.operations().iter().enumerate() {
        if let Some(final_file) = operation.final_file() {
            stage_final(transaction_directory, index, operation, final_file, faults)?;
        }
    }
    sync_directory(transaction_directory, RollbackStatus::NotRequired)?;
    persist_manifest(transaction_directory, &manifest.encode()?)?;
    faults.check(TransactionFaultPoint::AfterPreparedManifest).map_err(|error| {
        PatchError::io(PatchOperationContext::PersistManifest, RollbackStatus::NotRequired, error)
    })
}

fn stage_final(
    transaction_directory: &Path,
    index: usize,
    operation: &crate::PatchOperation,
    final_file: &crate::FinalFile,
    faults: &dyn FaultInjector,
) -> Result<(), PatchError> {
    let staged = staged_path(transaction_directory, index);
    let mut file =
        OpenOptions::new().write(true).create_new(true).open(&staged).map_err(|error| {
            PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
                .at(operation.path().clone())
        })?;
    file.write_all(final_file.bytes()).map_err(|error| {
        PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
            .at(operation.path().clone())
    })?;
    set_mode(&staged, final_file.mode())?;
    file.sync_all().map_err(|error| {
        PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
            .at(operation.path().clone())
    })?;
    faults.check(TransactionFaultPoint::AfterStageFinal).map_err(|error| {
        PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
    })
}

pub(super) fn persist_manifest(
    transaction_directory: &Path,
    bytes: &[u8],
) -> Result<(), PatchError> {
    let next = transaction_directory.join(NEXT_MANIFEST_FILE);
    let current = transaction_directory.join(MANIFEST_FILE);
    let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(&next).map_err(
        |error| {
            PatchError::io(
                PatchOperationContext::PersistManifest,
                RollbackStatus::NotRequired,
                error,
            )
        },
    )?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|error| {
        PatchError::io(PatchOperationContext::PersistManifest, RollbackStatus::NotRequired, error)
    })?;
    fs::rename(&next, &current).map_err(|error| {
        PatchError::io(PatchOperationContext::PersistManifest, RollbackStatus::NotRequired, error)
    })?;
    sync_directory(transaction_directory, RollbackStatus::NotRequired)
}

pub(super) fn cleanup_transaction(
    transaction_directory: &Path,
    transaction_root: &Path,
) -> Result<(), PatchError> {
    fs::remove_dir_all(transaction_directory).map_err(|error| {
        PatchError::io(PatchOperationContext::Cleanup, RollbackStatus::NotRequired, error)
    })?;
    sync_directory(transaction_root, RollbackStatus::NotRequired)
}

pub(super) fn staged_path(transaction_directory: &Path, index: usize) -> PathBuf {
    transaction_directory.join(format!("final-{index:04}"))
}

pub(super) fn backup_path(transaction_directory: &Path, index: usize) -> PathBuf {
    transaction_directory.join(format!("backup-{index:04}"))
}
