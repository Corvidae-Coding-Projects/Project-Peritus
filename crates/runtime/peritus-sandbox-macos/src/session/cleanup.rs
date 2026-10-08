//! Idempotent normal and pre-activation session teardown.

use peritus_process::NativeLaunchDescription;
use peritus_sandbox::{ObservationDisposition, ObservationKind};

use super::{MacosSession, ReleaseReport, SessionPhase};
use crate::{
    MacosError, MacosErrorKind, MacosOperation, ObservationEvent, ObservationStatus,
    RecoveryAction, session::adapter::lifecycle_error,
};

impl MacosSession {
    /// Releases all manifest/profile and closed-helper channel ownership idempotently.
    ///
    /// A prepared session may be abandoned before launch when C2 rejects its final validation.
    /// That path records release without inventing activation or termination.
    ///
    /// # Errors
    /// Rejects an active process tree or cleanup that cannot be proven complete.
    pub fn record_release(&mut self) -> Result<ReleaseReport, MacosError> {
        if self.phase == SessionPhase::Released {
            return Ok(ReleaseReport { cleanup: self.cleanup, already_released: true });
        }
        match self.phase {
            SessionPhase::Terminated | SessionPhase::Prepared => {}
            SessionPhase::Active | SessionPhase::Cancelling => {
                return Err(lifecycle_error("release requires observed termination"));
            }
            SessionPhase::Released => unreachable!("released session returned above"),
        }
        // A helper can materialize file-delivered secrets and then fail before C2 accepts the
        // activation acknowledgement. Prepared abandonment must therefore clean the same exact
        // destinations as normal termination; absent paths remain an idempotent success.
        if self.exec_status_cleanup_failed {
            return Err(cleanup_error(
                "execution-status monitor cleanup remains indeterminate",
            ));
        }
        if let Err(error) = self.exec_status.finish() {
            self.exec_status_cleanup_failed = true;
            self.recovery.record_execution_status_unavailable()?;
            return Err(error);
        }
        self.recovery.record_execution_status_released()?;
        self.cleanup.mark_support_joined();
        self.recovery.record_cleanup(self.cleanup)?;
        release_materialized_secret_files(&mut self.recovery)?;
        self.secrets.release().map_err(|_| cleanup_error("secret lease cleanup failed"))?;
        self.recovery.record_secrets_released()?;
        self.cleanup.mark_secrets_released();
        self.recovery.record_cleanup(self.cleanup)?;
        if self.proxy.is_some() {
            self.recovery.record_proxy_cleanup_required()?;
            if let Some(proxy) = self.proxy.as_mut()
                && let Err(error) = proxy.reconcile_shutdown()
            {
                return Err(
                    cleanup_error("managed proxy cleanup requires reconciliation")
                        .with_source(crate::error::network_source(&error)),
                );
            }
        }
        let mut proxy_released = self.cleanup;
        proxy_released.mark_proxy_released();
        self.recovery.record_proxy_released(proxy_released)?;
        self.cleanup = proxy_released;
        self.proxy = None;
        self.launch = NativeLaunchDescription::new(
            self.launch.command().clone(),
            self.launch.helper_identity(),
            self.launch.manifest().to_vec(),
            self.launch.manifest_digest(),
            self.launch.preparation_digest(),
        )
        .map_err(|_| cleanup_error("protected launch handles could not be released"))?;
        self.resource_monitor.release();
        self.recovery.record_native_released()?;
        self.cleanup.mark_native_released();
        self.recovery.record_cleanup(self.cleanup)?;
        if !self.cleanup.is_complete() {
            return Err(cleanup_error("one or more native resource families remain owned"));
        }
        self.phase = SessionPhase::Released;
        self.recovery.record_released()?;
        self.push_lifecycle(
            ObservationKind::Released,
            ObservationEvent::Released,
            ObservationDisposition::Completed,
            ObservationStatus::Completed,
        )?;
        Ok(ReleaseReport { cleanup: self.cleanup, already_released: false })
    }
}

fn release_materialized_secret_files(
    recovery: &mut crate::MacosRecoveryRecord,
) -> Result<(), MacosError> {
    while let Some(path) = recovery.materialized_secret_files().first().cloned() {
        remove_materialized_secret_file(&path)?;
        recovery
            .record_materialized_secret_file_released(&path)
            .map_err(|_| cleanup_error("materialized secret cleanup evidence could not be retained"))?;
    }
    Ok(())
}

fn remove_materialized_secret_file(path: &str) -> Result<(), MacosError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(cleanup_error("materialized secret file cleanup failed")),
    }
}

fn cleanup_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::CleanupIncomplete,
        MacosOperation::Release,
        RecoveryAction::RetryCleanup,
        detail,
    )
}

#[cfg(all(test, unix))]
mod tests {
    use peritus_sandbox::SandboxPath;

    use super::remove_materialized_secret_file;

    #[test]
    fn release_cleanup_removes_materialized_file_for_any_preterminal_phase() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("delivered.secret");
        std::fs::write(&path, b"material").unwrap();
        let logical_path = path.to_str().unwrap().replace('\\', "/");
        let sandbox_path = SandboxPath::new(logical_path).unwrap();
        let _manifest = crate::test_support::manifest_with_file_secret(sandbox_path);
        remove_materialized_secret_file(path.to_str().unwrap()).unwrap();
        assert!(!path.exists());
        remove_materialized_secret_file(path.to_str().unwrap()).unwrap();
    }
}
