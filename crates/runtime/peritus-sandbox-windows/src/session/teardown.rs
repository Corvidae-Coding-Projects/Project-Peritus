//! Independent backend-resource teardown and partial cleanup evidence.

use peritus_process::{NativeLaunchDescription, ProcessError};

use super::{WindowsSession, process_error};
use crate::{
    CleanupState, ReleaseReport, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery,
    WindowsRecoveryRecord,
};

impl WindowsSession {
    pub(super) fn release_owned_resources(&mut self) -> Result<ReleaseReport, ProcessError> {
        #[cfg(target_os = "windows")]
        let secret_file_failure = self
            .retain_secret_file_custody()
            .err()
            .or_else(|| remove_secret_files(&self.recovery).err());
        #[cfg(not(target_os = "windows"))]
        let secret_file_failure = remove_secret_files(&self.recovery).err();
        let secret_files_removed = secret_file_failure.is_none();
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

    #[cfg(target_os = "windows")]
    fn retain_secret_file_custody(&mut self) -> Result<(), ProcessError> {
        if self.recovery.phase() != crate::WindowsPhase::Prepared {
            return Ok(());
        }
        let Some(identities) = self.native_launch.windows_secret_files()? else {
            return Ok(());
        };
        let files = super::secret_file_recovery(&self.windows_launch, identities)?;
        self.recovery
            .retain_secret_files(files)
            .map_err(|error| process_error(&error))
    }
}

#[cfg(target_os = "windows")]
fn remove_secret_files(recovery: &WindowsRecoveryRecord) -> Result<bool, ProcessError> {
    use std::{
        fs::OpenOptions,
        os::windows::{
            fs::{MetadataExt as _, OpenOptionsExt as _},
            io::AsRawHandle as _,
        },
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FileDispositionInfo, FileIdInfo, GetFileInformationByHandleEx,
        SetFileInformationByHandle,
    };

    for expected in recovery.secret_files() {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .access_mode(DELETE | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let file = match options.open(expected.path().to_path_buf()) {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(secret_file_cleanup_error(
                "private secret file cannot be opened for exact cleanup",
            )),
        };
        let metadata = file.metadata().map_err(|_| {
            secret_file_cleanup_error("private secret file metadata cannot be observed")
        })?;
        let mut identity = FILE_ID_INFO::default();
        let identity_size = u32::try_from(core::mem::size_of::<FILE_ID_INFO>()).map_err(|_| {
            secret_file_cleanup_error("private secret file identity size overflowed")
        })?;
        // SAFETY: the File is live and identity is writable for the exact structure size.
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle().cast(),
                FileIdInfo,
                (&raw mut identity).cast(),
                identity_size,
            )
        } == 0
            || !metadata.is_file()
            || metadata.number_of_links() != Some(1)
            || metadata.len() != expected.payload_len()
            || identity.VolumeSerialNumber != expected.volume_serial()
            || identity.FileId.Identifier != expected.file_id()
        {
            return Err(secret_file_cleanup_error(
                "private secret file identity changed before cleanup",
            ));
        }
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        let disposition_size =
            u32::try_from(core::mem::size_of::<FILE_DISPOSITION_INFO>()).map_err(|_| {
                secret_file_cleanup_error("private secret cleanup record size overflowed")
            })?;
        // SAFETY: identity was checked through this same DELETE-capable handle.
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle().cast(),
                FileDispositionInfo,
                (&raw const disposition).cast(),
                disposition_size,
            )
        } == 0
        {
            return Err(secret_file_cleanup_error(
                "private secret file delete disposition cannot be armed",
            ));
        }
        drop(file);
        match expected.path().to_path_buf().try_exists() {
            Ok(false) => {}
            Ok(true) | Err(_) => {
                return Err(secret_file_cleanup_error(
                    "private secret file absence cannot be established after cleanup",
                ));
            }
        }
    }
    Ok(true)
}

#[cfg(not(target_os = "windows"))]
fn remove_secret_files(recovery: &WindowsRecoveryRecord) -> Result<bool, ProcessError> {
    if recovery.secret_files().is_empty() {
        Ok(true)
    } else {
        Err(secret_file_cleanup_error(
            "private secret file identity cannot be reconciled on this host",
        ))
    }
}

fn secret_file_cleanup_error(detail: &'static str) -> ProcessError {
    process_error(&WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Release,
        WindowsRecovery::RetryCleanup,
        detail,
    ))
}

fn cleanup_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Release,
        WindowsRecovery::RetryCleanup,
        detail,
    )
}
