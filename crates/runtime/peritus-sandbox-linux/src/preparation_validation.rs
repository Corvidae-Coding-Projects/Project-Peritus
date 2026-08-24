//! Validation of secret and proxy destinations before authorized native effects.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, MountPolicy};
use std::path::Path;

pub fn validate_secret_destinations(
    sandbox: &peritus_sandbox::CheckedSandboxPlan,
    execution: &peritus_process::ExecutionPlan,
    mount_policy: &MountPolicy,
) -> Result<(), LinuxError> {
    if !sandbox.requirements().network().is_empty()
        && execution
            .environment()
            .variables()
            .iter()
            .any(|variable| matches!(variable.name(), "HTTP_PROXY" | "HTTPS_PROXY"))
    {
        return Err(LinuxError::new(
            LinuxErrorKind::PreparationMismatch,
            LinuxOperation::Prepare,
            LinuxRecovery::CorrectRequest,
            "managed proxy environment collides with a literal assignment",
        ));
    }
    for requirement in sandbox.requirements().secrets() {
        match requirement.delivery() {
            peritus_sandbox::SecretDelivery::Environment(name) => {
                if execution
                    .environment()
                    .variables()
                    .iter()
                    .any(|variable| variable.name() == name.as_str())
                {
                    return Err(LinuxError::new(
                        LinuxErrorKind::PreparationMismatch,
                        LinuxOperation::Prepare,
                        LinuxRecovery::CorrectRequest,
                        "secret environment destination collides with a literal assignment",
                    ));
                }
            }
            peritus_sandbox::SecretDelivery::File(path) => {
                let native = Path::new(path.as_str());
                if !native.is_absolute()
                    || mount_policy
                        .protected_roots()
                        .iter()
                        .any(|protected| native.starts_with(protected))
                    || native.is_symlink()
                {
                    return Err(LinuxError::new(
                        LinuxErrorKind::Filesystem,
                        LinuxOperation::Prepare,
                        LinuxRecovery::CorrectRequest,
                        "secret file destination is non-native, protected, or aliases another path",
                    ));
                }
            }
            peritus_sandbox::SecretDelivery::BrokeredHandle(_) => {}
        }
    }
    Ok(())
}
