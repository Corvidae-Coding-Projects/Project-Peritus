//! Pre-effect validation of filesystem modes the platform can represent.

use super::{
    ErrorCode, PatchError, PatchOperationContext, PatchPlan, Preimage, RecoveryClass,
    RollbackStatus,
};

pub(super) fn validate_platform_modes(plan: &PatchPlan) -> Result<(), PatchError> {
    for operation in plan.operations() {
        let executable_preimage = matches!(
            operation.preimage(),
            Preimage::Present { mode: crate::FileMode::Executable, .. }
        );
        let executable_final = matches!(
            operation.postimage(),
            Preimage::Present { mode: crate::FileMode::Executable, .. }
        );
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
        for identity in [operation.preimage(), operation.postimage()] {
            if matches!(identity, Preimage::EmptyDirectory { mode } if mode.bits() != 0o555 && mode.bits() != 0o777)
            {
                return Err(PatchError::message(
                    ErrorCode::InvalidContent,
                    RecoveryClass::CorrectPatch,
                    PatchOperationContext::Plan,
                    RollbackStatus::NotRequired,
                    "directory permissions are not representable on this platform",
                )
                .at(operation.path().clone()));
            }
        }
    }
    Ok(())
}
