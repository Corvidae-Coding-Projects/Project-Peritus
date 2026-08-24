//! Close-on-exec status channel distinguishing helper failure from target exit.

use crate::{InheritedHandle, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use peritus_process::NativeProtectedHandle;
use peritus_types::Sha256Digest;

/// Exact manifest label for the helper-side close-on-exec status descriptor.
pub const EXEC_STATUS_LABEL: &str = "linux-helper-exec-status-v1";

/// Parent-side owner of one helper execution-status channel.
#[derive(Debug)]
pub struct ExecStatusOwner {
    #[cfg(target_os = "linux")]
    reader: std::os::unix::net::UnixStream,
}

/// Creates the parent reader and exact helper-side protected descriptor.
pub fn prepare() -> Result<(ExecStatusOwner, NativeProtectedHandle, InheritedHandle), LinuxError> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::OwnedFd;
        use std::time::Duration;

        let (reader, writer) = std::os::unix::net::UnixStream::pair().map_err(|error| {
            LinuxError::io(LinuxOperation::Prepare, "create exec status", &error)
        })?;
        reader.set_read_timeout(Some(Duration::from_secs(5))).map_err(|error| {
            LinuxError::io(LinuxOperation::Prepare, "bound exec status", &error)
        })?;
        let writer = std::fs::File::from(OwnedFd::from(writer));
        let handle = NativeProtectedHandle::from_file(EXEC_STATUS_LABEL, writer)
            .map_err(|_| status_error("exec status handle could not be protected"))?;
        let binding = InheritedHandle::new(handle.raw_handle(), EXEC_STATUS_LABEL.to_owned())?;
        Ok((ExecStatusOwner { reader }, handle, binding))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(LinuxError::new(
            LinuxErrorKind::UnsupportedHost,
            LinuxOperation::Prepare,
            LinuxRecovery::ConfigureHost,
            "Linux exec status is unavailable on this target",
        ))
    }
}

impl ExecStatusOwner {
    /// Observes close-on-exec success or one exact authenticated helper failure record.
    #[cfg(target_os = "linux")]
    pub fn observe(
        &mut self,
        manifest: Sha256Digest,
        preparation: Sha256Digest,
    ) -> Result<(), LinuxError> {
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

    /// Rejects observation when the Linux backend is compiled for another host.
    #[cfg(not(target_os = "linux"))]
    #[allow(
        clippy::needless_pass_by_ref_mut,
        clippy::unused_self,
        reason = "the cross-platform owner API retains its receiver while rejecting non-Linux use"
    )]
    pub fn observe(
        &mut self,
        _manifest: Sha256Digest,
        _preparation: Sha256Digest,
    ) -> Result<(), LinuxError> {
        Err(status_error("Linux exec status cannot be observed on this target"))
    }
}

#[cfg(target_os = "linux")]
#[allow(
    unsafe_code,
    reason = "one checked BorrowedFd converts the manifest-bound live descriptor into an owned close-on-exec duplicate"
)]
pub fn open_helper_attachment(descriptor: u64) -> Result<std::fs::File, LinuxError> {
    use std::os::fd::BorrowedFd;

    let descriptor = i32::try_from(descriptor)
        .map_err(|_| status_error("helper exec status descriptor exceeds Linux bounds"))?;
    // SAFETY: the helper has already confirmed this exact descriptor is live and uniquely labelled
    // in the checksummed manifest. The borrow exists only for `dup` and cannot close the source.
    let borrowed = unsafe { BorrowedFd::borrow_raw(descriptor) };
    let duplicate = nix::unistd::dup(borrowed)
        .map_err(|_| status_error("helper exec status descriptor could not be duplicated"))?;
    nix::unistd::close(descriptor)
        .map_err(|_| status_error("helper exec status source could not be closed"))?;
    let flags = nix::fcntl::fcntl(&duplicate, nix::fcntl::FcntlArg::F_GETFD)
        .map_err(|_| status_error("helper exec status flags could not be read"))?;
    let flags = nix::fcntl::FdFlag::from_bits_retain(flags) | nix::fcntl::FdFlag::FD_CLOEXEC;
    nix::fcntl::fcntl(&duplicate, nix::fcntl::FcntlArg::F_SETFD(flags))
        .map_err(|_| status_error("helper exec status could not be made close-on-exec"))?;
    Ok(std::fs::File::from(duplicate))
}

#[cfg(target_os = "linux")]
pub fn report_helper_failure(
    writer: &mut std::fs::File,
    manifest: Sha256Digest,
    preparation: Sha256Digest,
) -> Result<(), LinuxError> {
    use std::io::Write;

    writer
        .write_all(failure_record(manifest, preparation).as_bytes())
        .and_then(|()| writer.flush())
        .map_err(|error| LinuxError::io(LinuxOperation::Activate, "report exec failure", &error))
}

#[cfg(target_os = "linux")]
fn failure_record(manifest: Sha256Digest, preparation: Sha256Digest) -> Sha256Digest {
    peritus_process::native_target_exec_failed_record(manifest, preparation)
}

fn status_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Helper,
        LinuxOperation::Activate,
        LinuxRecovery::CancelAndReap,
        detail,
    )
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{fs::File, os::fd::OwnedFd, os::unix::net::UnixStream, time::Duration};

    use peritus_types::Sha256Digest;

    use super::{ExecStatusOwner, report_helper_failure};

    #[test]
    fn close_on_exec_eof_is_success() {
        let (reader, writer) = UnixStream::pair().expect("status pair");
        reader.set_read_timeout(Some(Duration::from_secs(1))).expect("timeout");
        drop(writer);
        let mut owner = ExecStatusOwner { reader };
        owner.observe(Sha256Digest::new([1; 32]), Sha256Digest::new([2; 32])).expect("EOF");
    }

    #[test]
    fn digest_bound_failure_is_typed() {
        let (reader, writer) = UnixStream::pair().expect("status pair");
        reader.set_read_timeout(Some(Duration::from_secs(1))).expect("timeout");
        let mut writer = File::from(OwnedFd::from(writer));
        let manifest = Sha256Digest::new([3; 32]);
        let preparation = Sha256Digest::new([4; 32]);
        report_helper_failure(&mut writer, manifest, preparation).expect("failure record");
        drop(writer);
        let mut owner = ExecStatusOwner { reader };
        assert!(owner.observe(manifest, preparation).is_err());
    }
}
