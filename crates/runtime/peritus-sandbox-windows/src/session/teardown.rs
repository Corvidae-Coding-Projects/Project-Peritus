//! Independent backend-resource teardown and partial cleanup evidence.

use peritus_process::{NativeLaunchDescription, ProcessError};

use super::{WindowsSession, process_error};
use crate::{
    CleanupState, ReleaseReport, WindowsError, WindowsErrorKind, WindowsLaunchDescription,
    WindowsOperation, WindowsRecovery,
};

impl WindowsSession {
    pub(super) fn release_owned_resources(&mut self) -> Result<ReleaseReport, ProcessError> {
        let acl_failure = self.acl.restore().err().map(|error| process_error(&error));
        let acl_restored = acl_failure.is_none() && self.acl.restored();
        let filter_failure = self.filter.release().err().map(|error| process_error(&error));
        let network_filter_removed = filter_failure.is_none();
        self.filter_cleanup = if network_filter_removed {
            CleanupState::Complete
        } else {
            CleanupState::RetryRequired
        };
        let proxy_failure = self.release_proxy().err();
        let proxy_joined = proxy_failure.is_none();
        let secret_failure = self.secrets.as_mut().and_then(|secrets| {
            secrets.release().err().map(|source| {
                process_error(
                    &WindowsError::new(
                        WindowsErrorKind::Secret,
                        WindowsOperation::Release,
                        WindowsRecovery::RetryCleanup,
                        "exact secret delivery cleanup failed",
                    )
                    .with_source(crate::error::secret_source(&source)),
                )
            })
        });
        let secret_delivery_released = secret_failure.is_none();
        self.secret_cleanup = if secret_delivery_released {
            CleanupState::Complete
        } else {
            CleanupState::RetryRequired
        };
        let secret_file_failure = remove_secret_files(&self.windows_launch).err();
        let secret_files_removed = secret_file_failure.is_none();
        let handle_failure = self.release_protected_handles().err();
        let handles_closed = handle_failure.is_none();
        let report = ReleaseReport {
            acl_restored,
            secret_files_removed,
            helper_reaped: true,
            handles_closed,
            proxy_joined,
            network_filter_removed,
        };
        if let Some(error) = [
            acl_failure,
            filter_failure,
            proxy_failure,
            secret_failure,
            secret_file_failure,
            handle_failure,
        ]
        .into_iter()
        .flatten()
        .next()
        {
            return Err(error);
        }
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
            CleanupState::Pending | CleanupState::RetryRequired => {
                let Some(proxy) = self.proxy.as_mut() else {
                    self.proxy_cleanup = CleanupState::RetryRequired;
                    return Err(process_error(&cleanup_error(
                        "managed proxy ownership disappeared before teardown",
                    )));
                };
                match proxy.reconcile_shutdown() {
                    Ok(()) => {
                        self.proxy_cleanup = CleanupState::Complete;
                        self.proxy = None;
                        Ok(true)
                    }
                    Err(source) => {
                        self.proxy_cleanup = CleanupState::RetryRequired;
                        Err(process_error(
                            &cleanup_error("managed proxy teardown failed")
                                .with_source(crate::error::network_source(&source)),
                        ))
                    }
                }
            }
        }
    }

    fn release_protected_handles(&mut self) -> Result<(), ProcessError> {
        if self.native_launch.protected_handles().is_empty() {
            return Ok(());
        }
        let replacement = NativeLaunchDescription::new_paged(
            self.native_launch.command().clone(),
            self.native_launch.helper_identity().to_owned(),
            self.native_launch.manifest_pages().map(<[u8]>::to_vec).collect(),
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
