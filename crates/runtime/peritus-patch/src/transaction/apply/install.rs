use std::{fs, path::Path};

use crate::{
    ErrorCode, PatchError, PatchOperation, PatchOperationContext, PatchPlan, RecoveryClass,
    RollbackStatus,
};

use super::super::{
    filesystem::{
        checked_target_path, create_directory, observation_matches, observe_target_cancellable,
        preserve_replacement_permissions, sync_directory,
    },
    manifest::{FileIdentity, Manifest, TransactionPhase},
    storage::{backup_path, persist_manifest, staged_path},
};
use super::validation::verify_manifest_postimages;
use super::{FaultInjector, TransactionFaultPoint};

struct InstallPaths<'a> {
    target: &'a Path,
    parent: &'a Path,
    transaction_directory: &'a Path,
    index: usize,
}

pub(super) fn install_all(
    workspace: &Path,
    transaction_directory: &Path,
    plan: &PatchPlan,
    manifest: &mut Manifest,
    faults: &dyn FaultInjector,
    mutated: &mut bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, PatchError> {
    check_fault(
        faults,
        TransactionFaultPoint::AfterInstallingManifest,
        PatchOperationContext::PersistManifest,
        RollbackStatus::NotRequired,
    )?;
    for directory in &manifest.created_directories {
        if !*mutated {
            check_cancelled(cancelled)?;
        }
        create_directory(workspace, directory, mutated)?;
        check_fault(
            faults,
            TransactionFaultPoint::AfterCreateDirectory,
            PatchOperationContext::InstallFinal,
            RollbackStatus::Indeterminate,
        )?;
    }
    for (index, operation) in plan.operations().iter().enumerate() {
        install_operation(
            workspace,
            transaction_directory,
            index,
            operation,
            faults,
            mutated,
            cancelled,
        )?;
    }
    check_fault(
        faults,
        TransactionFaultPoint::BeforeVerifyResult,
        PatchOperationContext::VerifyResult,
        RollbackStatus::Indeterminate,
    )?;
    verify_manifest_postimages(workspace, manifest)?;
    manifest.phase = TransactionPhase::Installed;
    let installed = manifest.encode()?;
    persist_manifest(transaction_directory, &installed)?;
    Ok(installed)
}

fn install_operation(
    workspace: &Path,
    transaction_directory: &Path,
    index: usize,
    operation: &PatchOperation,
    faults: &dyn FaultInjector,
    mutated: &mut bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PatchError> {
    let never_cancelled = || false;
    let observation_cancellation = if *mutated { &never_cancelled } else { cancelled };
    let observed = observe_target_cancellable(
        workspace,
        operation.path(),
        PatchOperationContext::InspectPreimage,
        RollbackStatus::Indeterminate,
        observation_cancellation,
    )?;
    if !observation_matches(observed, FileIdentity::from_preimage(operation.preimage())) {
        return Err(PatchError::message(
            ErrorCode::PreimageMismatch,
            RecoveryClass::ReinspectWorkspace,
            PatchOperationContext::InspectPreimage,
            RollbackStatus::Indeterminate,
            "target changed after transaction preparation",
        )
        .at(operation.path().clone()));
    }
    if !*mutated {
        check_cancelled(cancelled)?;
    }
    let target = checked_target_path(
        workspace,
        operation.path(),
        PatchOperationContext::InstallFinal,
        RollbackStatus::Indeterminate,
    )?;
    let parent = target
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::InstallFinal))?;
    let paths = InstallPaths { target: &target, parent, transaction_directory, index };
    if matches!(
        operation.kind(),
        crate::PatchOperationKind::Replace | crate::PatchOperationKind::Delete
    ) {
        backup_original(&paths, operation.path(), faults, mutated)?;
    }
    if operation.final_file().is_some() {
        install_final(&paths, operation, faults, mutated)?;
    }
    Ok(())
}

fn backup_original(
    paths: &InstallPaths<'_>,
    path: &crate::WorkspacePath,
    faults: &dyn FaultInjector,
    mutated: &mut bool,
) -> Result<(), PatchError> {
    fs::rename(paths.target, backup_path(paths.transaction_directory, paths.index)).map_err(
        |error| {
            PatchError::io(
                PatchOperationContext::BackupOriginal,
                RollbackStatus::Indeterminate,
                error,
            )
            .at(path.clone())
        },
    )?;
    *mutated = true;
    check_fault(
        faults,
        TransactionFaultPoint::AfterBackupOriginal,
        PatchOperationContext::BackupOriginal,
        RollbackStatus::Indeterminate,
    )?;
    sync_with_fault(faults, paths.parent, RollbackStatus::Indeterminate)?;
    sync_directory(paths.transaction_directory, RollbackStatus::Indeterminate)
}

fn install_final(
    paths: &InstallPaths<'_>,
    operation: &PatchOperation,
    faults: &dyn FaultInjector,
    mutated: &mut bool,
) -> Result<(), PatchError> {
    if operation.kind() == crate::PatchOperationKind::Replace {
        let mode = operation
            .final_file()
            .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::InstallFinal))?
            .mode();
        preserve_replacement_permissions(
            &staged_path(paths.transaction_directory, paths.index),
            &backup_path(paths.transaction_directory, paths.index),
            mode,
        )?;
    }
    fs::rename(staged_path(paths.transaction_directory, paths.index), paths.target).map_err(
        |error| {
            PatchError::io(
                PatchOperationContext::InstallFinal,
                RollbackStatus::Indeterminate,
                error,
            )
            .at(operation.path().clone())
        },
    )?;
    *mutated = true;
    check_fault(
        faults,
        TransactionFaultPoint::AfterInstallFinal,
        PatchOperationContext::InstallFinal,
        RollbackStatus::Indeterminate,
    )?;
    sync_with_fault(faults, paths.parent, RollbackStatus::Indeterminate)?;
    sync_directory(paths.transaction_directory, RollbackStatus::Indeterminate)
}

fn sync_with_fault(
    faults: &dyn FaultInjector,
    directory: &Path,
    rollback: RollbackStatus,
) -> Result<(), PatchError> {
    check_fault(
        faults,
        TransactionFaultPoint::BeforeDirectorySync,
        PatchOperationContext::SynchronizeDirectory,
        rollback,
    )?;
    sync_directory(directory, rollback)
}

pub(super) fn check_fault(
    faults: &dyn FaultInjector,
    point: TransactionFaultPoint,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
) -> Result<(), PatchError> {
    faults.check(point).map_err(|error| PatchError::io(operation, rollback, error))
}

fn check_cancelled(cancelled: &dyn Fn() -> bool) -> Result<(), PatchError> {
    if cancelled() { Err(cancellation_error()) } else { Ok(()) }
}

pub(super) const fn cancellation_error() -> PatchError {
    PatchError::message(
        ErrorCode::Cancelled,
        RecoveryClass::Retry,
        PatchOperationContext::Cancellation,
        RollbackStatus::NotRequired,
        "cancellation was observed before workspace mutation",
    )
}
