//! Independent backend-resource teardown and partial cleanup evidence.

use peritus_process::{NativeLaunchDescription, ProcessError};

use super::{WindowsSession, process_error};
use crate::{
    CleanupState, ReleaseReport, WindowsError, WindowsErrorKind, WindowsLaunchDescription,
    WindowsOperation, WindowsRecovery,
};

impl WindowsSession {
    pub(super) fn release_owned_resources(&mut self) -> Result<ReleaseReport, ProcessError> {
        let acl_restored = self.acl.restore().is_ok() && self.acl.restored();
        let network_filter_removed = self.filter.release().unwrap_or(false);
        let proxy_joined = self.release_proxy().unwrap_or(false);
        let secret_delivery_released =
            self.secrets.as_mut().is_none_or(|secrets| secrets.release().is_ok());
        let secret_files_removed = remove_secret_files(&self.windows_launch).unwrap_or(false);
        let handles_closed = self.release_protected_handles().is_ok();
        let report = ReleaseReport {
            acl_restored,
            secret_files_removed,
            helper_reaped: true,
            handles_closed,
            proxy_joined,
            network_filter_removed,
        };
        if !secret_delivery_released || !report.complete() {
            return Err(process_error(&WindowsError::new(
                WindowsErrorKind::RecoveryIndeterminate,
                WindowsOperation::Release,
                WindowsRecovery::RetryCleanup,
                "Windows release could not prove every native resource absent",
            )));
        }
        Ok(report)
    }

    fn release_proxy(&mut self) -> Result<bool, ProcessError> {
        match self.proxy_cleanup {
            CleanupState::Complete => Ok(true),
            CleanupState::RetryRequired => Err(process_error(&cleanup_error(
                "managed proxy teardown previously failed and requires reconciliation",
            ))),
            CleanupState::Pending => {
                let Some(proxy) = self.proxy.take() else {
                    self.proxy_cleanup = CleanupState::RetryRequired;
                    return Err(process_error(&cleanup_error(
                        "managed proxy ownership disappeared before teardown",
                    )));
                };
                match proxy.shutdown() {
                    Ok(result) if result.workers_joined() => {
                        self.proxy_cleanup = CleanupState::Complete;
                        Ok(true)
                    }
                    Ok(_) | Err(_) => {
                        self.proxy_cleanup = CleanupState::RetryRequired;
                        Err(process_error(&cleanup_error("managed proxy teardown failed")))
                    }
                }
            }
        }
    }

    fn release_protected_handles(&mut self) -> Result<(), ProcessError> {
        if self.native_launch.protected_handles().is_empty() {
            return Ok(());
        }
        let replacement = NativeLaunchDescription::new(
            self.native_launch.command().clone(),
            self.native_launch.helper_identity().to_owned(),
            self.native_launch.manifest().to_vec(),
            self.native_launch.manifest_digest(),
            self.native_launch.preparation_digest(),
        )?;
        let prior = core::mem::replace(&mut self.native_launch, replacement);
        drop(prior);
        Ok(())
    }
}

fn remove_secret_files(launch: &WindowsLaunchDescription) -> Result<bool, ProcessError> {
    for handle in launch.manifest().secret_handles() {
        if let crate::SecretHandleDestination::File(path) = handle.destination() {
            let native =
                crate::WindowsPath::from_sandbox(launch.manifest().working_directory(), path)
                    .map_err(|error| process_error(&error))?
                    .to_path_buf();
            if std::fs::remove_file(&native).is_err() && native.exists() {
                return Err(process_error(&WindowsError::new(
                    WindowsErrorKind::Secret,
                    WindowsOperation::Release,
                    WindowsRecovery::RetryCleanup,
                    "private secret file could not be removed",
                )));
            }
        }
    }
    Ok(true)
}

fn cleanup_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Release,
        WindowsRecovery::RetryCleanup,
        detail,
    )
}
