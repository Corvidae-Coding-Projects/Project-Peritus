//! Atomic multi-file application using staged finals and durable backups.

mod install;
mod validation;
use install::{cancellation_error, check_fault, install_all};
#[cfg(not(unix))]
use validation::validate_platform_modes;
#[cfg(unix)]
use validation::validate_volume_modes;
use validation::verify_plan_preimages;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    ErrorCode, PatchError, PatchOperationContext, PatchPlan, RecoveryClass, RollbackStatus,
};

use super::{
    AppliedPatch, FaultInjector, NoFaults, TransactionFaultPoint,
    filesystem::{discover_missing_directories, sync_directory},
    manifest::{Manifest, TransactionPhase},
    recover::rollback_workspace,
    roots::prepare_roots,
    storage::{cleanup_transaction, persist_manifest, prepare_transaction},
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
    apply_patch_with_completion(workspace_root, transaction_root, plan, |_| Ok(()))
}

/// Applies a patch and records its terminal result before removing transaction evidence.
///
/// `completion` receives the durable applied result after every postimage is verified, or `None`
/// after an application failure has been rolled back and verified. Preparation failures that
/// precede a terminal workspace outcome leave their durable transaction manifest available for
/// recovery and do not invoke the callback.
///
/// # Errors
///
/// Returns a patch or callback error. An indeterminate transaction remains available for recovery.
pub fn apply_patch_with_completion(
    workspace_root: impl AsRef<Path>,
    transaction_root: impl AsRef<Path>,
    plan: &PatchPlan,
    completion: impl FnOnce(Option<&AppliedPatch>) -> Result<(), PatchError>,
) -> Result<AppliedPatch, PatchError> {
    apply_with_completion(
        workspace_root.as_ref(),
        transaction_root.as_ref(),
        plan,
        &NoFaults,
        &|| false,
        completion,
    )
    .and_then(|applied| applied.ok_or_else(cancellation_error))
}

/// Applies a patch while allowing cancellation before the first workspace target mutation.
///
/// Cancellation after that point is deferred until the transaction reaches a verified terminal
/// state. When cancellation is observed at a safe point, the completion callback records the
/// unchanged workspace as rolled back, transaction evidence is removed, and this returns `Ok(None)`.
///
/// # Errors
///
/// Returns a patch or callback error. An indeterminate transaction remains available for recovery.
pub fn apply_patch_with_completion_and_cancellation(
    workspace_root: impl AsRef<Path>,
    transaction_root: impl AsRef<Path>,
    plan: &PatchPlan,
    cancelled: impl Fn() -> bool,
    completion: impl FnOnce(Option<&AppliedPatch>) -> Result<(), PatchError>,
) -> Result<Option<AppliedPatch>, PatchError> {
    apply_with_completion(
        workspace_root.as_ref(),
        transaction_root.as_ref(),
        plan,
        &NoFaults,
        &cancelled,
        completion,
    )
}

#[cfg(test)]
pub(super) fn apply_with_faults(
    workspace_root: &Path,
    transaction_root: &Path,
    plan: &PatchPlan,
    faults: &dyn FaultInjector,
) -> Result<AppliedPatch, PatchError> {
    apply_with_completion(workspace_root, transaction_root, plan, faults, &|| false, |_| Ok(()))
        .and_then(|applied| applied.ok_or_else(cancellation_error))
}

fn apply_with_completion(
    workspace_root: &Path,
    transaction_root: &Path,
    plan: &PatchPlan,
    faults: &dyn FaultInjector,
    cancelled: &dyn Fn() -> bool,
    completion: impl FnOnce(Option<&AppliedPatch>) -> Result<(), PatchError>,
) -> Result<Option<AppliedPatch>, PatchError> {
    let mut completion = Some(completion);
    let Some(prepared) = prepare_transaction_for_apply(
        workspace_root,
        transaction_root,
        plan,
        faults,
        cancelled,
        &mut completion,
    )?
    else {
        return Ok(None);
    };
    finish_application(prepared, plan, faults, cancelled, &mut completion)
}

struct PreparedTransaction {
    workspace: PathBuf,
    transaction_root: PathBuf,
    directory: PathBuf,
    manifest: Manifest,
}

fn prepare_transaction_for_apply<F>(
    workspace_root: &Path,
    transaction_root: &Path,
    plan: &PatchPlan,
    faults: &dyn FaultInjector,
    cancelled: &dyn Fn() -> bool,
    completion: &mut Option<F>,
) -> Result<Option<PreparedTransaction>, PatchError>
where
    F: FnOnce(Option<&AppliedPatch>) -> Result<(), PatchError>,
{
    if cancelled() {
        complete_action(completion, None)?;
        return Ok(None);
    }
    check_fault(
        faults,
        TransactionFaultPoint::BeforePrepare,
        PatchOperationContext::Prepare,
        RollbackStatus::NotRequired,
    )?;
    #[cfg(not(unix))]
    validate_platform_modes(plan)?;
    let roots = prepare_roots(workspace_root, transaction_root)?;
    #[cfg(unix)]
    validate_volume_modes(&roots.transaction_root, plan)?;
    if cancelled() {
        complete_action(completion, None)?;
        return Ok(None);
    }
    if let Err(error) = verify_plan_preimages(&roots.workspace, plan, cancelled) {
        if error.code() == ErrorCode::Cancelled {
            complete_action(completion, None)?;
            return Ok(None);
        }
        return Err(error);
    }
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

    if cancelled() {
        complete_action(completion, None)?;
        cleanup_cancelled_transaction(&transaction_directory, &roots.transaction_root)?;
        return Ok(None);
    }

    let manifest = Manifest::from_plan(plan, created_directories);
    if let Err(error) = prepare_transaction(
        &roots.workspace,
        &transaction_directory,
        plan,
        &manifest,
        faults,
        cancelled,
    ) {
        if error.code() == ErrorCode::Cancelled {
            complete_action(completion, None)?;
            cleanup_cancelled_transaction(&transaction_directory, &roots.transaction_root)?;
            return Ok(None);
        }
        return Err(error);
    }

    if cancelled() {
        complete_action(completion, None)?;
        cleanup_cancelled_transaction(&transaction_directory, &roots.transaction_root)?;
        return Ok(None);
    }

    Ok(Some(PreparedTransaction {
        workspace: roots.workspace,
        transaction_root: roots.transaction_root,
        directory: transaction_directory,
        manifest,
    }))
}

fn finish_application<F>(
    prepared: PreparedTransaction,
    plan: &PatchPlan,
    faults: &dyn FaultInjector,
    cancelled: &dyn Fn() -> bool,
    completion: &mut Option<F>,
) -> Result<Option<AppliedPatch>, PatchError>
where
    F: FnOnce(Option<&AppliedPatch>) -> Result<(), PatchError>,
{
    let PreparedTransaction { workspace, transaction_root, directory, mut manifest } = prepared;
    manifest.phase = TransactionPhase::Installing;
    let installing = manifest.encode()?;
    persist_manifest(&directory, &installing)?;
    let mut mutated = false;
    let application =
        install_all(&workspace, &directory, plan, &mut manifest, faults, &mut mutated, cancelled);

    let installed_manifest = match application {
        Ok(installed) => installed,
        Err(error) => {
            if !mutated {
                if error.code() == ErrorCode::Cancelled {
                    complete_action(completion, None)?;
                    cleanup_cancelled_transaction(&directory, &transaction_root)?;
                    return Ok(None);
                }
                return Err(error.with_rollback(RollbackStatus::NotRequired));
            }
            if check_fault(
                faults,
                TransactionFaultPoint::BeforeRollback,
                PatchOperationContext::Rollback,
                RollbackStatus::Indeterminate,
            )
            .is_err()
                || rollback_workspace(&workspace, &directory, &manifest).is_err()
            {
                return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
            }
            if let Err(receipt_error) = complete_action(completion, None) {
                return Err(receipt_error.with_rollback(RollbackStatus::Indeterminate));
            }
            if cleanup_transaction(&directory, &transaction_root).is_err() {
                return Err(PatchError::message(
                    ErrorCode::InterruptedTransaction,
                    RecoveryClass::RecoverTransaction,
                    PatchOperationContext::Cleanup,
                    RollbackStatus::Restored,
                    "workspace rollback was verified but transaction cleanup remains pending",
                ));
            }
            return Err(error.with_rollback(RollbackStatus::Restored));
        }
    };

    let applied = AppliedPatch::new(plan.identity(), installed_manifest, false);
    if let Err(error) = complete_action(completion, Some(&applied)) {
        return Err(error.with_rollback(RollbackStatus::Indeterminate));
    }

    let cleanup_pending = check_fault(
        faults,
        TransactionFaultPoint::BeforeCleanup,
        PatchOperationContext::Cleanup,
        RollbackStatus::NotRequired,
    )
    .is_err()
        || cleanup_transaction(&directory, &transaction_root).is_err();
    Ok(Some(AppliedPatch::new(plan.identity(), applied.installed_manifest, cleanup_pending)))
}

fn complete_action<F>(
    completion: &mut Option<F>,
    applied: Option<&AppliedPatch>,
) -> Result<(), PatchError>
where
    F: FnOnce(Option<&AppliedPatch>) -> Result<(), PatchError>,
{
    completion.take().map_or_else(
        || Err(PatchError::indeterminate(PatchOperationContext::PersistManifest)),
        |completion| completion(applied),
    )
}

fn cleanup_cancelled_transaction(
    transaction_directory: &Path,
    transaction_root: &Path,
) -> Result<(), PatchError> {
    cleanup_transaction(transaction_directory, transaction_root).map_err(|_| {
        PatchError::message(
            ErrorCode::InterruptedTransaction,
            RecoveryClass::RecoverTransaction,
            PatchOperationContext::Cleanup,
            RollbackStatus::NotRequired,
            "cancellation was recorded but prepared transaction cleanup remains pending",
        )
    })
}
