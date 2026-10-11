use super::{
    ErrorCode, Manifest, PatchError, PatchOperationContext, PatchPlan, Path, RecoveryClass,
    RollbackStatus,
};
#[cfg(unix)]
use super::{fs, io, sync_directory};
use crate::Preimage;
use crate::transaction::{
    filesystem::{Observation, observation_matches, observe_target, observe_target_cancellable},
    manifest::FileIdentity,
};

#[cfg(unix)]
pub(super) fn validate_volume_modes(
    transaction_root: &Path,
    plan: &PatchPlan,
) -> Result<(), PatchError> {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    static PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);
    let needs_executable = plan.operations().iter().any(|operation| {
        matches!(operation.preimage(), Preimage::Present { mode: crate::FileMode::Executable, .. })
            || operation.final_file().is_some_and(|file| file.mode() == crate::FileMode::Executable)
    });
    if !needs_executable {
        return Ok(());
    }

    let (path, file) = loop {
        let sequence = PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = transaction_root.join(format!(".mode-probe-{}-{sequence}", std::process::id()));
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(PatchError::io(
                    PatchOperationContext::Plan,
                    RollbackStatus::NotRequired,
                    error,
                ));
            }
        }
    };
    let check = (|| {
        let mut permissions = file.metadata()?.permissions();
        permissions.set_mode((permissions.mode() & 0o666) | 0o700);
        file.set_permissions(permissions)?;
        file.sync_all()?;
        let mode = fs::metadata(&path)?.permissions().mode();
        if mode & 0o111 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "transaction volume does not preserve executable file modes",
            ));
        }
        Ok(())
    })();
    drop(file);
    if let Err(error) = check {
        fs::remove_file(&path).map_err(|cleanup_error| {
            PatchError::io(PatchOperationContext::Plan, RollbackStatus::NotRequired, cleanup_error)
        })?;
        sync_directory(transaction_root, RollbackStatus::NotRequired)?;
        return Err(PatchError::io(
            PatchOperationContext::Plan,
            RollbackStatus::NotRequired,
            error,
        ));
    }
    fs::remove_file(&path).map_err(|error| {
        PatchError::io(PatchOperationContext::Plan, RollbackStatus::NotRequired, error)
    })?;
    sync_directory(transaction_root, RollbackStatus::NotRequired)
}

#[cfg(not(unix))]
pub(super) fn validate_platform_modes(plan: &PatchPlan) -> Result<(), PatchError> {
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

pub(super) fn verify_plan_preimages(
    workspace: &Path,
    plan: &PatchPlan,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PatchError> {
    for operation in plan.operations() {
        let observed = observe_target_cancellable(
            workspace,
            operation.path(),
            PatchOperationContext::InspectPreimage,
            RollbackStatus::NotRequired,
            cancelled,
        )?;
        let expected = FileIdentity::from_preimage(operation.preimage());
        if !observation_matches(observed, expected) {
            let (code, detail) = match (observed, operation.preimage()) {
                (Observation::Absent, Preimage::Present { .. }) => {
                    (ErrorCode::PreimageMissing, "required preimage file is absent")
                }
                (Observation::Present(_), Preimage::Absent) => {
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

pub(super) fn verify_manifest_postimages(
    workspace: &Path,
    manifest: &Manifest,
) -> Result<(), PatchError> {
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
