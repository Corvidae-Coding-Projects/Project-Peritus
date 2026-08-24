//! Atomic multi-file application using staged finals and durable backups.

use std::{fs, io, path::Path};

use crate::{
    ErrorCode, PatchError, PatchOperationContext, PatchPlan, Preimage, RecoveryClass,
    RollbackStatus,
};

use super::{
    AppliedPatch, FaultInjector, NoFaults, TransactionFaultPoint,
    filesystem::{
        Observation, checked_target_path, create_directory, discover_missing_directories,
        observation_matches, observe_target, sync_directory,
    },
    manifest::{FileIdentity, Manifest, TransactionPhase},
    recover::rollback_workspace,
    roots::prepare_roots,
    storage::{
        backup_path, cleanup_transaction, persist_manifest, prepare_transaction, staged_path,
    },
};

/// Applies a checked plan as one recoverable multi-file filesystem transaction.
///
/// The transaction root must be a separate directory on the same filesystem as the workspace.
/// Only a [`PatchPlan`] is accepted; callers cannot pass an unchecked [`crate::PatchSet`].
///
/// ```compile_fail
/// # use peritus_patch::{PatchSet, apply_patch};
/// # fn demo(patch: &PatchSet) {
/// let _ = apply_patch("workspace", "transactions", patch);
/// # }
/// ```
///
/// # Errors
///
/// Returns a typed preimage, safety, I/O, or rollback error. An indeterminate rollback leaves the
/// durable transaction directory in place for [`super::recover_transaction`].
pub fn apply_patch(
    workspace_root: impl AsRef<Path>,
    transaction_root: impl AsRef<Path>,
    plan: &PatchPlan,
) -> Result<AppliedPatch, PatchError> {
    apply_with_faults(workspace_root.as_ref(), transaction_root.as_ref(), plan, &NoFaults)
}

pub(super) fn apply_with_faults(
    workspace_root: &Path,
    transaction_root: &Path,
    plan: &PatchPlan,
    faults: &dyn FaultInjector,
) -> Result<AppliedPatch, PatchError> {
    check_fault(
        faults,
        TransactionFaultPoint::BeforePrepare,
        PatchOperationContext::Prepare,
        RollbackStatus::NotRequired,
    )?;
    #[cfg(not(unix))]
    validate_platform_modes(plan)?;
    let roots = prepare_roots(workspace_root, transaction_root)?;
    verify_plan_preimages(&roots.workspace, plan)?;
    let created_directories = discover_missing_directories(
        &roots.workspace,
        plan.operations().iter().map(|operation| operation.path().clone()),
    )?;
    let transaction_directory =
        roots.transaction_root.join(format!("txn-{}", plan.identity().to_hex()));
    match fs::create_dir(&transaction_directory) {
        Ok(()) => sync_directory(&roots.transaction_root, RollbackStatus::NotRequired)?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(PatchError::message(
                ErrorCode::InterruptedTransaction,
                RecoveryClass::RecoverTransaction,
                PatchOperationContext::Prepare,
                RollbackStatus::NotRequired,
                "the patch transaction already exists and must be recovered",
            ));
        }
        Err(error) => {
            return Err(PatchError::io(
                PatchOperationContext::Prepare,
                RollbackStatus::NotRequired,
                error,
            ));
        }
    }

    let mut manifest = Manifest::from_plan(plan, created_directories);
    if let Err(error) = prepare_transaction(&transaction_directory, plan, &manifest, faults) {
        let _cleanup_result = cleanup_transaction(&transaction_directory, &roots.transaction_root);
        return Err(error);
    }

    manifest.phase = TransactionPhase::Installing;
    let installing = manifest.encode()?;
    if let Err(error) = persist_manifest(&transaction_directory, &installing) {
        let _cleanup_result = cleanup_transaction(&transaction_directory, &roots.transaction_root);
        return Err(error);
    }
    let mut mutated = false;
    let application = install_all(
        &roots.workspace,
        &transaction_directory,
        plan,
        &mut manifest,
        faults,
        &mut mutated,
    );

    let installed_manifest = match application {
        Ok(installed) => installed,
        Err(error) => {
            if !mutated {
                let _cleanup_result =
                    cleanup_transaction(&transaction_directory, &roots.transaction_root);
                return Err(error.with_rollback(RollbackStatus::NotRequired));
            }
            if check_fault(
                faults,
                TransactionFaultPoint::BeforeRollback,
                PatchOperationContext::Rollback,
                RollbackStatus::Indeterminate,
            )
            .is_err()
                || rollback_workspace(&roots.workspace, &transaction_directory, &manifest).is_err()
            {
                return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
            }
            let _cleanup_result =
                cleanup_transaction(&transaction_directory, &roots.transaction_root);
            return Err(error.with_rollback(RollbackStatus::Restored));
        }
    };

    let cleanup_pending = check_fault(
        faults,
        TransactionFaultPoint::BeforeCleanup,
        PatchOperationContext::Cleanup,
        RollbackStatus::NotRequired,
    )
    .is_err()
        || cleanup_transaction(&transaction_directory, &roots.transaction_root).is_err();
    Ok(AppliedPatch::new(plan.identity(), installed_manifest, cleanup_pending))
}

#[cfg(not(unix))]
fn validate_platform_modes(plan: &PatchPlan) -> Result<(), PatchError> {
    for operation in plan.operations() {
        let executable_preimage = matches!(
            operation.preimage(),
            Preimage::Present { mode: crate::FileMode::Executable, .. }
        );
        let executable_final = operation
            .final_file()
            .is_some_and(|final_file| final_file.mode() == crate::FileMode::Executable);
        if executable_preimage || executable_final {
            return Err(PatchError::message(
                ErrorCode::InvalidContent,
                RecoveryClass::CorrectPatch,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "executable file mode is unsupported on this platform",
            )
            .at(operation.path().clone()));
        }
    }
    Ok(())
}

fn install_all(
    workspace: &Path,
    transaction_directory: &Path,
    plan: &PatchPlan,
    manifest: &mut Manifest,
    faults: &dyn FaultInjector,
    mutated: &mut bool,
) -> Result<Vec<u8>, PatchError> {
    check_fault(
        faults,
        TransactionFaultPoint::AfterInstallingManifest,
        PatchOperationContext::PersistManifest,
        RollbackStatus::NotRequired,
    )?;
    for directory in &manifest.created_directories {
        create_directory(workspace, directory, mutated)?;
        check_fault(
            faults,
            TransactionFaultPoint::AfterCreateDirectory,
            PatchOperationContext::InstallFinal,
            RollbackStatus::Indeterminate,
        )?;
    }
    for (index, operation) in plan.operations().iter().enumerate() {
        install_operation(workspace, transaction_directory, index, operation, faults, mutated)?;
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
    operation: &crate::PatchOperation,
    faults: &dyn FaultInjector,
    mutated: &mut bool,
) -> Result<(), PatchError> {
    let observed = observe_target(
        workspace,
        operation.path(),
        PatchOperationContext::InspectPreimage,
        RollbackStatus::Indeterminate,
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
    let target = checked_target_path(
        workspace,
        operation.path(),
        PatchOperationContext::InstallFinal,
        RollbackStatus::Indeterminate,
    )?;
    let parent = target
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::InstallFinal))?;
    if matches!(
        operation.kind(),
        crate::PatchOperationKind::Replace | crate::PatchOperationKind::Delete
    ) {
        let backup = backup_path(transaction_directory, index);
        fs::rename(&target, &backup).map_err(|error| {
            PatchError::io(
                PatchOperationContext::BackupOriginal,
                RollbackStatus::Indeterminate,
                error,
            )
            .at(operation.path().clone())
        })?;
        *mutated = true;
        check_fault(
            faults,
            TransactionFaultPoint::AfterBackupOriginal,
            PatchOperationContext::BackupOriginal,
            RollbackStatus::Indeterminate,
        )?;
        sync_with_fault(faults, parent, RollbackStatus::Indeterminate)?;
        sync_directory(transaction_directory, RollbackStatus::Indeterminate)?;
    }
    if operation.final_file().is_some() {
        fs::rename(staged_path(transaction_directory, index), &target).map_err(|error| {
            PatchError::io(
                PatchOperationContext::InstallFinal,
                RollbackStatus::Indeterminate,
                error,
            )
            .at(operation.path().clone())
        })?;
        *mutated = true;
        check_fault(
            faults,
            TransactionFaultPoint::AfterInstallFinal,
            PatchOperationContext::InstallFinal,
            RollbackStatus::Indeterminate,
        )?;
        sync_with_fault(faults, parent, RollbackStatus::Indeterminate)?;
        sync_directory(transaction_directory, RollbackStatus::Indeterminate)?;
    }
    Ok(())
}

fn verify_plan_preimages(workspace: &Path, plan: &PatchPlan) -> Result<(), PatchError> {
    for operation in plan.operations() {
        let observed = observe_target(
            workspace,
            operation.path(),
            PatchOperationContext::InspectPreimage,
            RollbackStatus::NotRequired,
        )?;
        let expected = FileIdentity::from_preimage(operation.preimage());
        if !observation_matches(observed, expected) {
            let (code, detail) = match (observed, operation.preimage()) {
                (Observation::Absent, Preimage::Present { .. }) => {
                    (ErrorCode::PreimageMissing, "required preimage file is absent")
                }
                (Observation::Present(_) | Observation::Oversized, Preimage::Absent) => {
                    (ErrorCode::PreimageUnexpected, "create target already exists")
                }
                _ => {
                    (ErrorCode::PreimageMismatch, "file bytes, size, or mode do not match preimage")
                }
            };
            return Err(PatchError::message(
                code,
                RecoveryClass::ReinspectWorkspace,
                PatchOperationContext::InspectPreimage,
                RollbackStatus::NotRequired,
                detail,
            )
            .at(operation.path().clone()));
        }
    }
    Ok(())
}

fn verify_manifest_postimages(workspace: &Path, manifest: &Manifest) -> Result<(), PatchError> {
    for entry in &manifest.entries {
        let observed = observe_target(
            workspace,
            &entry.path,
            PatchOperationContext::VerifyResult,
            RollbackStatus::Indeterminate,
        )?;
        if !observation_matches(observed, entry.postimage) {
            return Err(PatchError::message(
                ErrorCode::InvalidContent,
                RecoveryClass::FenceWorkspace,
                PatchOperationContext::VerifyResult,
                RollbackStatus::Indeterminate,
                "installed target does not match the declared postimage",
            )
            .at(entry.path.clone()));
        }
    }
    Ok(())
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

fn check_fault(
    faults: &dyn FaultInjector,
    point: TransactionFaultPoint,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
) -> Result<(), PatchError> {
    faults.check(point).map_err(|error| PatchError::io(operation, rollback, error))
}
