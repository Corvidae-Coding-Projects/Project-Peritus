//! Independent backend-resource teardown and partial cleanup evidence.

use peritus_process::{NativeLaunchDescription, ProcessError};

use super::{WindowsSession, process_error};
use crate::{
    CleanupFailure, CleanupState, ReleaseReport, WindowsError, WindowsErrorKind, WindowsOperation,
    WindowsRecovery, WindowsRecoveryRecord,
    recovery::RecoveryCleanupDimension,
};

impl WindowsSession {
    pub(super) fn release_owned_resources(&mut self) -> Result<ReleaseReport, ProcessError> {
        let mut helper_failure = self
            .record_cleanup(RecoveryCleanupDimension::Helper)
            .err();
        helper_failure = self.retain_cleanup_failure(
            RecoveryCleanupDimension::Helper,
            helper_failure,
        );
        self.helper_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().helper_reaped(),
            self.recovery.cleanup_failures().helper(),
        );
        let mut job_failure = self
            .release_containment_job()
            .and_then(|()| self.record_cleanup(RecoveryCleanupDimension::Job))
            .err();
        job_failure = self.retain_cleanup_failure(RecoveryCleanupDimension::Job, job_failure);
        self.job_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().job_closed(),
            self.recovery.cleanup_failures().job(),
        );
        #[cfg(target_os = "windows")]
        let mut secret_file_failure = self
            .retain_secret_file_custody()
            .err()
            .or_else(|| remove_secret_files(&self.recovery).err());
        #[cfg(not(target_os = "windows"))]
        let mut secret_file_failure = remove_secret_files(&self.recovery).err();
        if secret_file_failure.is_none() {
            secret_file_failure = self
                .record_cleanup(RecoveryCleanupDimension::SecretFiles)
                .err();
        }
        secret_file_failure = self.retain_cleanup_failure(
            RecoveryCleanupDimension::SecretFiles,
            secret_file_failure,
        );
        self.secret_file_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().secret_files_removed(),
            self.recovery.cleanup_failures().secret_files(),
        );
        let mut acl_failure = self.acl.restore().err().map(|error| process_error(&error));
        if acl_failure.is_none() && self.acl.restored() {
            acl_failure = self.record_cleanup(RecoveryCleanupDimension::Acl).err();
        }
        acl_failure = self.retain_cleanup_failure(RecoveryCleanupDimension::Acl, acl_failure);
        let mut filter_failure = self.filter.release().err().map(|error| process_error(&error));
        if filter_failure.is_none() {
            filter_failure = self
                .record_cleanup(RecoveryCleanupDimension::NetworkFilter)
                .err();
        }
        filter_failure = self.retain_cleanup_failure(
            RecoveryCleanupDimension::NetworkFilter,
            filter_failure,
        );
        self.filter_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().network_filter_removed(),
            self.recovery.cleanup_failures().filter(),
        );
        let (mut proxy_failure, proxy_terminal) = match self.release_proxy() {
            Ok(terminal) => (None, terminal),
            Err(error) => (Some(error), None),
        };
        if proxy_failure.is_none() {
            proxy_failure = self.record_cleanup(RecoveryCleanupDimension::Proxy).err();
        }
        if let Some(terminal) = proxy_terminal
            && let Err(error) = self.recovery.mark_cleanup_failure(
                RecoveryCleanupDimension::Proxy,
                CleanupFailure::from_windows_error(&terminal),
            )
        {
            proxy_failure = Some(process_error(&error));
        }
        proxy_failure = self.retain_cleanup_failure(
            RecoveryCleanupDimension::Proxy,
            proxy_failure,
        );
        self.proxy_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().proxy_joined(),
            self.recovery.cleanup_failures().proxy(),
        );
        let mut secret_failure = self.secrets.as_mut().and_then(|secrets| {
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
        if secret_failure.is_none() {
            secret_failure = self
                .record_cleanup(RecoveryCleanupDimension::SecretDelivery)
                .err();
        }
        secret_failure = self.retain_cleanup_failure(
            RecoveryCleanupDimension::SecretDelivery,
            secret_failure,
        );
        self.secret_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().secret_delivery_released(),
            self.recovery.cleanup_failures().secret_delivery(),
        );
        let mut handle_failure = self.release_protected_handles().err();
        if handle_failure.is_none() {
            handle_failure = self.record_cleanup(RecoveryCleanupDimension::Handles).err();
        }
        handle_failure = self.retain_cleanup_failure(
            RecoveryCleanupDimension::Handles,
            handle_failure,
        );
        self.handle_cleanup = cleanup_attempt_state(
            self.recovery.cleanup().handles_closed(),
            self.recovery.cleanup_failures().handles(),
        );
        let cleanup = self.recovery.cleanup();
        let report = ReleaseReport {
            job_closed: cleanup.job_closed(),
            helper_reaped: cleanup.helper_reaped(),
            acl_restored: cleanup.acl_restored(),
            secret_files_removed: cleanup.secret_files_removed(),
            secret_delivery_released: cleanup.secret_delivery_released(),
            handles_closed: cleanup.handles_closed(),
            proxy_joined: cleanup.proxy_joined(),
            network_filter_removed: cleanup.network_filter_removed(),
        };
        if let Some(error) = [
            helper_failure,
            job_failure,
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
        if !report.complete() {
            return Err(process_error(&WindowsError::new(
                WindowsErrorKind::RecoveryIndeterminate,
                WindowsOperation::Release,
                WindowsRecovery::ReconcileCleanup,
                "Windows release could not prove every native resource absent",
            )));
        }
        Ok(report)
    }

    fn record_cleanup(
        &mut self,
        dimension: RecoveryCleanupDimension,
    ) -> Result<(), ProcessError> {
        self.recovery
            .mark_cleanup(dimension)
            .map_err(|error| process_error(&error))
    }

    fn retain_cleanup_failure(
        &mut self,
        dimension: RecoveryCleanupDimension,
        failure: Option<ProcessError>,
    ) -> Option<ProcessError> {
        let failure = failure?;
        let cause = CleanupFailure::from_process_error(&failure);
        match self.recovery.mark_cleanup_failure(dimension, cause) {
            Ok(()) => Some(failure),
            Err(error) => Some(process_error(&error)),
        }
    }

    fn release_containment_job(&mut self) -> Result<(), ProcessError> {
        if self.recovery.cleanup().job_closed() {
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        {
            let expected = self.recovery.identity().job_identity();
            if !self.native_launch.retains_windows_job(expected) {
                return Err(process_error(&cleanup_error(
                    "retained Job Object custody disappeared before teardown",
                )));
            }
            if !self
                .native_launch
                .release_protected_handle(peritus_process::NATIVE_WINDOWS_JOB_HANDLE_LABEL)
            {
                return Err(process_error(&cleanup_error(
                    "retained Job Object handle cannot be closed exactly",
                )));
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(process_error(&cleanup_error(
                "retained Job Object cannot be reconciled on this host",
            )))
        }
    }

    fn release_proxy(&mut self) -> Result<Option<WindowsError>, ProcessError> {
        match self.proxy_cleanup {
            CleanupState::Complete => Ok(None),
            CleanupState::Pending
            | CleanupState::RetryRequired
            | CleanupState::ReconciliationRequired => {
                let Some(proxy) = self.proxy.as_mut() else {
                    self.proxy_cleanup = CleanupState::ReconciliationRequired;
                    return Err(process_error(&proxy_reconciliation_error(
                        "managed proxy ownership disappeared before teardown",
                    )));
                };
                match proxy.reconcile_shutdown() {
                    Ok(receipt) => {
                        let terminal = receipt.terminal_failure().map(|source| {
                            let recovery = if source.recovery()
                                == peritus_network::RecoveryClass::Reconcile
                            {
                                WindowsRecovery::ReconcileCleanup
                            } else {
                                WindowsRecovery::RetryCleanup
                            };
                            WindowsError::new(
                                WindowsErrorKind::Network,
                                WindowsOperation::Release,
                                recovery,
                                "managed proxy owner terminated with a retained failure",
                            )
                            .with_source(crate::error::network_source(&source))
                        });
                        self.proxy_cleanup = CleanupState::Complete;
                        self.proxy = None;
                        Ok(terminal)
                    }
                    Err(source) => {
                        self.proxy_cleanup = CleanupState::ReconciliationRequired;
                        Err(process_error(
                            &proxy_reconciliation_error(
                                "managed proxy join requires explicit reconciliation",
                            )
                                .with_source(crate::error::network_source(&source)),
                        ))
                    }
                }
            }
        }
    }

    fn release_protected_handles(&mut self) -> Result<(), ProcessError> {
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

fn cleanup_attempt_state(complete: bool, failure: Option<&CleanupFailure>) -> CleanupState {
    if complete {
        CleanupState::Complete
    } else if failure.is_some_and(CleanupFailure::requires_reconciliation) || failure.is_none() {
        CleanupState::ReconciliationRequired
    } else {
        CleanupState::RetryRequired
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
        WindowsRecovery::ReconcileCleanup,
        detail,
    )
}

fn proxy_reconciliation_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Recover,
        WindowsRecovery::ReconcileCleanup,
        detail,
    )
}
