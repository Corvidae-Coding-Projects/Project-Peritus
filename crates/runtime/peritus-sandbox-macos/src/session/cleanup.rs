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
    /// That path performs complete cleanup without recording normal target lifecycle events.
    ///
    /// # Errors
    /// Rejects an active process tree or cleanup that cannot be proven complete.
    pub fn record_release(&mut self) -> Result<ReleaseReport, MacosError> {
        if self.phase == SessionPhase::Released {
            return Ok(ReleaseReport { cleanup: self.cleanup, already_released: true });
        }
        let normal_completion = match self.phase {
            SessionPhase::Terminated => true,
            SessionPhase::Active | SessionPhase::Cancelling => {
                return Err(lifecycle_error("release requires observed termination"));
            }
            SessionPhase::Prepared | SessionPhase::Released => false,
        };
        // A helper can materialize file-delivered secrets and then fail before C2 accepts the
        // activation acknowledgement. Prepared abandonment must therefore clean the same exact
        // destinations as normal termination; absent paths remain an idempotent success.
        remove_materialized_secret_files(&self.manifest)?;
        self.secrets.release().map_err(|_| cleanup_error("secret lease cleanup failed"))?;
        self.cleanup.mark_secrets_released();
        self.recovery.record_cleanup(self.cleanup)?;
        if self.proxy_cleanup_failed {
            return Err(cleanup_error("managed proxy cleanup remains indeterminate"));
        }
        if let Some(proxy) = self.proxy.take()
            && proxy.shutdown().is_err()
        {
            self.proxy_cleanup_failed = true;
            return Err(cleanup_error("managed proxy cleanup failed"));
        }
        self.cleanup.mark_proxy_released();
        self.recovery.record_cleanup(self.cleanup)?;
        self.launch = NativeLaunchDescription::new(
            self.launch.command().clone(),
            self.launch.helper_identity(),
            self.launch.manifest().to_vec(),
            self.launch.manifest_digest(),
            self.launch.preparation_digest(),
        )
        .map_err(|_| cleanup_error("protected launch handles could not be released"))?;
        self.cleanup.mark_native_released();
        self.recovery.record_cleanup(self.cleanup)?;
        if !self.cleanup.is_complete() {
            return Err(cleanup_error("one or more native resource families remain owned"));
        }
        self.phase = SessionPhase::Released;
        if normal_completion {
            self.push_lifecycle(
                ObservationKind::Released,
                ObservationEvent::Released,
                ObservationDisposition::Completed,
                ObservationStatus::Completed,
            )?;
        }
        Ok(ReleaseReport { cleanup: self.cleanup, already_released: false })
    }
}

fn remove_materialized_secret_files(manifest: &crate::HelperManifest) -> Result<(), MacosError> {
    for secret in manifest.secrets() {
        let crate::SecretHandleDestination::File(path) = secret.destination() else {
            continue;
        };
        match std::fs::remove_file(path.as_str()) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(cleanup_error("materialized secret file cleanup failed")),
        }
    }
    Ok(())
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

    use super::remove_materialized_secret_files;

    #[test]
    fn release_cleanup_removes_materialized_file_for_any_preterminal_phase() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("delivered.secret");
        std::fs::write(&path, b"material").unwrap();
        let logical_path = path.to_str().unwrap().replace('\\', "/");
        let sandbox_path = SandboxPath::new(logical_path).unwrap();
        let manifest = crate::test_support::manifest_with_file_secret(sandbox_path);
        remove_materialized_secret_files(&manifest).unwrap();
        assert!(!path.exists());
        remove_materialized_secret_files(&manifest).unwrap();
    }
}
