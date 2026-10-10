//! Close-on-exec status channel distinguishing helper failure from target termination.

use peritus_process::NativeProtectedHandle;
use peritus_types::Sha256Digest;

use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction};

/// Exact manifest label for the helper-side close-on-exec status descriptor.
pub const EXEC_STATUS_LABEL: &str = "macos-helper-exec-status-v1";

/// Parent-side owner of one helper execution-status channel.
#[derive(Debug)]
pub(crate) struct ExecStatusOwner {
    #[cfg(unix)]
    reader: std::os::unix::net::UnixStream,
}

/// Creates the parent reader and exact helper-side protected descriptor.
pub(crate) fn prepare() -> Result<(ExecStatusOwner, NativeProtectedHandle), MacosError> {
    #[cfg(unix)]
    {
        use std::{os::fd::OwnedFd, time::Duration};

        let (reader, writer) = std::os::unix::net::UnixStream::pair()
            .map_err(|_| status_error("helper exec status channel could not be created"))?;
        reader
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|_| status_error("helper exec status channel could not be bounded"))?;
        let writer = std::fs::File::from(OwnedFd::from(writer));
        let handle = NativeProtectedHandle::from_file(EXEC_STATUS_LABEL, writer)
            .map_err(|_| status_error("helper exec status handle could not be protected"))?;
        Ok((ExecStatusOwner { reader }, handle))
    }
    #[cfg(not(unix))]
    {
        Err(MacosError::new(
            MacosErrorKind::UnsupportedHost,
            MacosOperation::Prepare,
            RecoveryAction::SelectSupportedBackend,
            "macOS helper exec status is unavailable on this target",
        ))
    }
}

impl ExecStatusOwner {
    /// Observes close-on-exec success or one exact authenticated helper failure record.
    #[cfg(unix)]
    pub(crate) fn observe(
        &mut self,
        manifest: Sha256Digest,
        preparation: Sha256Digest,
    ) -> Result<(), MacosError> {
        use std::io::Read;

        let mut bytes = Vec::new();
        self.reader
            .by_ref()
            .take(33)
            .read_to_end(&mut bytes)
            .map_err(|_| status_error("helper exec status timed out or could not be read"))?;
        if bytes.is_empty() {
            return Ok(());
        }
        if bytes.as_slice() == failure_record(manifest, preparation).as_bytes() {
            return Err(status_error("native helper could not exec the literal target"));
        }
        Err(status_error("native helper exec status record is malformed"))
    }

    /// Rejects observation when the macOS backend is compiled for a non-Unix host.
    #[cfg(not(unix))]
    #[allow(
        clippy::needless_pass_by_ref_mut,
        clippy::unused_self,
        reason = "the cross-platform owner API retains its receiver while rejecting non-Unix use"
    )]
    pub(crate) fn observe(
        &mut self,
        _manifest: Sha256Digest,
        _preparation: Sha256Digest,
    ) -> Result<(), MacosError> {
        Err(status_error("macOS helper exec status cannot be observed on this target"))
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn report_helper_failure(
    descriptor: u32,
    manifest: Sha256Digest,
    preparation: Sha256Digest,
) -> Result<(), MacosError> {
    crate::runner::write_exec_status(descriptor, failure_record(manifest, preparation).as_bytes())
}

#[cfg(unix)]
fn failure_record(manifest: Sha256Digest, preparation: Sha256Digest) -> Sha256Digest {
    peritus_process::native_target_exec_failed_record(manifest, preparation)
}

fn status_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Activate,
        RecoveryAction::CancelAndReap,
        detail,
    )
}
