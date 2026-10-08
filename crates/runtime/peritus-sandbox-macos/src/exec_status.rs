//! Framed execution acknowledgement under the process owner's native custody.

use peritus_process::{NativeLaunchDescription, NativeProtectedHandle, ProcessTreeIdentity};
use peritus_types::Sha256Digest;

use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction};

/// Exact manifest label for the helper-side execution-status descriptor.
pub const EXEC_STATUS_LABEL: &str = "macos-helper-exec-status-v2";

const FRAME_PAYLOAD_BYTES: usize = 1 + Sha256Digest::LENGTH;
const FRAME_BYTES: usize = 4 + FRAME_PAYLOAD_BYTES;
const EXECUTED_TAG: u8 = 1;
const EXEC_FAILED_TAG: u8 = 2;

/// Parent-side owner of one helper execution-status channel and its kernel exec observer.
#[derive(Debug)]
pub(crate) struct ExecStatusOwner {
    #[cfg(unix)]
    reader: Option<std::os::unix::net::UnixStream>,
    launch_writer: LaunchWriterHandoff,
    frame: [u8; FRAME_BYTES],
    frame_offset: usize,
    frame_validation: FrameValidation,
    cancelled: bool,
    #[cfg(target_os = "macos")]
    monitor_writer: Option<std::os::unix::net::UnixStream>,
    #[cfg(target_os = "macos")]
    monitor: Option<std::thread::JoinHandle<Result<(), MacosError>>>,
}

/// Creates the parent reader, kernel-monitor writer, and exact helper-side protected descriptor.
pub(crate) fn prepare() -> Result<(ExecStatusOwner, NativeProtectedHandle), MacosError> {
    #[cfg(unix)]
    {
        use std::os::fd::OwnedFd;

        let (reader, writer) = std::os::unix::net::UnixStream::pair().map_err(|_| {
            status_error("helper execution acknowledgement channel could not be created")
        })?;
        reader.set_nonblocking(true).map_err(|_| {
            status_error(
                "helper execution acknowledgement reader could not be made nonblocking",
            )
        })?;
        #[cfg(target_os = "macos")]
        let monitor_writer = writer
            .try_clone()
            .map_err(|_| status_error("execution monitor writer could not be retained"))?;
        let writer = std::fs::File::from(OwnedFd::from(writer));
        let handle = NativeProtectedHandle::from_file(EXEC_STATUS_LABEL, writer).map_err(|_| {
            status_error("helper execution acknowledgement handle could not be protected")
        })?;
        let launch_writer = LaunchWriterHandoff::LaunchOwned(handle.raw_handle());
        Ok((
            ExecStatusOwner {
                reader: Some(reader),
                launch_writer,
                frame: [0; FRAME_BYTES],
                frame_offset: 0,
                frame_validation: FrameValidation::Pending,
                cancelled: false,
                #[cfg(target_os = "macos")]
                monitor_writer: Some(monitor_writer),
                #[cfg(target_os = "macos")]
                monitor: None,
            },
            handle,
        ))
    }
    #[cfg(not(unix))]
    {
        Err(MacosError::new(
            MacosErrorKind::UnsupportedHost,
            MacosOperation::Prepare,
            RecoveryAction::SelectSupportedBackend,
            "macOS helper execution acknowledgement is unavailable on this target",
        ))
    }
}

impl ExecStatusOwner {
    /// Transfers the exact launch writer into retained execution-status custody once.
    ///
    /// The physical writer must close before observation so helper disappearance remains
    /// observable as EOF. The retained handoff identity makes that irreversible close resumable:
    /// a repeated activation continues with the existing reader instead of requiring another
    /// launch handle or dispatch.
    pub(crate) fn handoff_launch_writer(
        &mut self,
        launch: &mut NativeLaunchDescription,
    ) -> bool {
        let expected_handle = match self.launch_writer {
            LaunchWriterHandoff::LaunchOwned(handle) => handle,
            LaunchWriterHandoff::ObservationOwned(handle) => {
                return !launch.protected_handles().iter().any(|candidate| {
                    candidate.label() == EXEC_STATUS_LABEL
                        || candidate.raw_handle() == handle
                });
            }
        };
        let exact_writer_present = launch.protected_handles().iter().any(|handle| {
            handle.label() == EXEC_STATUS_LABEL
                && handle.raw_handle() == expected_handle
                && handle.payload_len().is_none()
        });
        if !exact_writer_present || !launch.release_protected_handle(EXEC_STATUS_LABEL) {
            return false;
        }
        self.launch_writer = LaunchWriterHandoff::ObservationOwned(expected_handle);
        true
    }

    /// Reports whether the launch still owns the exact status writer.
    pub(crate) const fn launch_writer_handoff_required(&self) -> bool {
        matches!(self.launch_writer, LaunchWriterHandoff::LaunchOwned(_))
    }

    /// Starts the one-use kernel observer before any manifest bytes can release the helper.
    #[cfg(all(target_os = "macos", not(test)))]
    #[allow(
        unsafe_code,
        reason = "kqueue registration is the narrow macOS helper-exec observation boundary"
    )]
    pub(crate) fn observe_spawned(
        &mut self,
        tree: ProcessTreeIdentity,
        manifest: Sha256Digest,
        preparation: Sha256Digest,
    ) -> Result<(), MacosError> {
        use std::os::fd::{FromRawFd as _, OwnedFd};

        if self.monitor.is_some() || self.monitor_writer.is_none() {
            return Err(status_error("execution monitor was already consumed"));
        }
        if tree.root_pid() == 0
            || tree.process_group() != Some(tree.root_pid())
            || !tree.complete_containment()
        {
            return Err(status_error(
                "execution monitor received incomplete process-tree identity",
            ));
        }
        // SAFETY: kqueue creates a new descriptor owned by this activation monitor.
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return Err(status_error("execution monitor queue could not be created"));
        }
        // SAFETY: `queue` is a newly created descriptor transferred into this sole owner.
        let queue = unsafe { OwnedFd::from_raw_fd(queue) };
        let registration = libc::kevent {
            ident: usize::try_from(tree.root_pid()).unwrap_or(usize::MAX),
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: libc::NOTE_EXEC | libc::NOTE_EXIT,
            data: 0,
            udata: core::ptr::null_mut(),
        };
        // SAFETY: `queue` is live and `registration` names one initialized change record. No
        // output event list is supplied during registration.
        if unsafe {
            libc::kevent(
                std::os::fd::AsRawFd::as_raw_fd(&queue),
                &raw const registration,
                1,
                core::ptr::null_mut(),
                0,
                core::ptr::null(),
            )
        } < 0
        {
            return Err(status_error(
                "execution monitor could not register the helper identity",
            ));
        }
        let writer = self
            .monitor_writer
            .take()
            .ok_or_else(|| status_error("execution monitor writer is unavailable"))?;
        self.monitor = Some(
            std::thread::Builder::new()
                .name("peritus-macos-exec-observer".to_owned())
                .spawn(move || monitor_exec(queue, writer, manifest, preparation))
                .map_err(|_| status_error("execution monitor thread could not be created"))?,
        );
        Ok(())
    }

    /// Rejects native spawn observation outside macOS.
    #[cfg(all(not(target_os = "macos"), not(test)))]
    #[allow(clippy::unused_self, reason = "the cross-platform session retains one API")]
    pub(crate) fn observe_spawned(
        &mut self,
        _tree: ProcessTreeIdentity,
        _manifest: Sha256Digest,
        _preparation: Sha256Digest,
    ) -> Result<(), MacosError> {
        Err(status_error(
            "macOS execution monitoring is unavailable on this target",
        ))
    }

    #[cfg(test)]
    pub(crate) fn observe_spawned(
        &mut self,
        tree: ProcessTreeIdentity,
        _manifest: Sha256Digest,
        _preparation: Sha256Digest,
    ) -> Result<(), MacosError> {
        if tree.root_pid() == 0
            || tree.start_token().is_none()
            || tree.process_group() != Some(tree.root_pid())
            || !tree.complete_containment()
        {
            return Err(status_error(
                "execution monitor received incomplete process-tree identity",
            ));
        }
        Ok(())
    }

    /// Reads one exact execution frame while the durable owner still permits activation.
    #[cfg(unix)]
    pub(crate) fn observe_while(
        &mut self,
        manifest: Sha256Digest,
        preparation: Sha256Digest,
        mut should_continue: impl FnMut() -> bool,
    ) -> Result<(), MacosError> {
        use std::io::{ErrorKind, Read as _};

        if self.cancelled || !should_continue() {
            self.cancelled = true;
            return Err(status_cancelled());
        }
        match self.frame_validation {
            FrameValidation::Pending => {}
            FrameValidation::Executed => return Ok(()),
            FrameValidation::Rejected(detail) => return Err(status_error(detail)),
        }
        while self.frame_offset < self.frame.len() {
            if !should_continue() {
                self.cancelled = true;
                return Err(status_cancelled());
            }
            let reader = self.reader.as_mut().ok_or_else(|| {
                status_error("helper execution acknowledgement owner was already released")
            })?;
            match reader.read(&mut self.frame[self.frame_offset..]) {
                Ok(0) => {
                    let detail = if self.frame_offset == 0 {
                        "helper disappeared before target execution was acknowledged"
                    } else {
                        "helper execution acknowledgement frame is truncated"
                    };
                    return Err(status_error(detail));
                }
                Ok(count) => self.frame_offset += count,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    std::thread::yield_now();
                }
                Err(_) => {
                    return Err(status_error(
                        "helper execution acknowledgement could not be read",
                    ));
                }
            }
        }
        self.frame_validation = decode_frame(self.frame, manifest, preparation);
        match self.frame_validation {
            FrameValidation::Pending => Err(status_error(
                "helper execution acknowledgement could not be validated",
            )),
            FrameValidation::Executed => Ok(()),
            FrameValidation::Rejected(detail) => Err(status_error(detail)),
        }
    }

    /// Rejects observation when the macOS backend is compiled for a non-Unix host.
    #[cfg(not(unix))]
    #[allow(
        clippy::needless_pass_by_ref_mut,
        clippy::unused_self,
        reason = "the cross-platform owner API retains its receiver while rejecting non-Unix use"
    )]
    pub(crate) fn observe_while(
        &mut self,
        _manifest: Sha256Digest,
        _preparation: Sha256Digest,
        _should_continue: impl FnMut() -> bool,
    ) -> Result<(), MacosError> {
        Err(status_error(
            "macOS execution acknowledgement cannot be observed on this target",
        ))
    }

    /// Joins the exact kernel observer after C2 established helper-tree quiescence.
    pub(crate) fn finish(&mut self) -> Result<(), MacosError> {
        #[cfg(target_os = "macos")]
        drop(self.monitor_writer.take());
        #[cfg(target_os = "macos")]
        let monitor_result = if let Some(monitor) = self.monitor.take() {
            match monitor.join() {
                Ok(result) => result,
                Err(_) => Err(status_error("execution monitor thread panicked")),
            }
        } else {
            Ok(())
        };
        #[cfg(not(target_os = "macos"))]
        let monitor_result = Ok(());
        #[cfg(unix)]
        drop(self.reader.take());
        monitor_result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchWriterHandoff {
    LaunchOwned(u64),
    ObservationOwned(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameValidation {
    Pending,
    Executed,
    Rejected(&'static str),
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "kevent is the narrow macOS helper-exec observation boundary"
)]
fn monitor_exec(
    queue: std::os::fd::OwnedFd,
    mut writer: std::os::unix::net::UnixStream,
    manifest: Sha256Digest,
    preparation: Sha256Digest,
) -> Result<(), MacosError> {
    use std::{io::Write as _, mem::MaybeUninit, os::fd::AsRawFd as _};

    loop {
        let mut event = MaybeUninit::<libc::kevent>::uninit();
        // SAFETY: `queue` is live and the one-element output points to writable storage. The
        // blocking wait is owned by this joinable custody thread and is released by exec or exit.
        let observed = unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                core::ptr::null(),
                0,
                event.as_mut_ptr(),
                1,
                core::ptr::null(),
            )
        };
        if observed < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(status_error(
                "execution monitor could not observe helper state",
            ));
        }
        if observed == 0 {
            continue;
        }
        // SAFETY: kevent reported that it initialized the single output record.
        let event = unsafe { event.assume_init() };
        if event.fflags & libc::NOTE_EXEC != 0 {
            let frame = frame(EXECUTED_TAG, success_record(manifest, preparation));
            writer
                .write_all(&frame)
                .and_then(|()| writer.flush())
                .map_err(|_| {
                    status_error("execution acknowledgement could not be published")
                })?;
            return Ok(());
        }
        if event.fflags & libc::NOTE_EXIT != 0 {
            return Ok(());
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn report_helper_failure_while(
    descriptor: u32,
    manifest: Sha256Digest,
    preparation: Sha256Digest,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), MacosError> {
    crate::runner::write_status_while(
        descriptor,
        &frame(EXEC_FAILED_TAG, failure_record(manifest, preparation)),
        should_continue,
    )
}

#[cfg(all(test, unix))]
#[allow(
    unsafe_code,
    reason = "the test adapter duplicates only its fixture-owned execution-status descriptor"
)]
pub(crate) fn report_test_success(
    descriptor: u32,
    manifest: Sha256Digest,
    preparation: Sha256Digest,
) -> Result<(), MacosError> {
    use std::{io::Write as _, os::fd::{FromRawFd as _, OwnedFd}};

    // SAFETY: the fixture retains the manifest-bound descriptor for the duration of this call.
    let duplicate = unsafe { libc::fcntl(descriptor.cast_signed(), libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(status_error(
            "test execution acknowledgement descriptor could not be duplicated",
        ));
    }
    // SAFETY: F_DUPFD_CLOEXEC returned a new uniquely owned descriptor above.
    let duplicate = unsafe { OwnedFd::from_raw_fd(duplicate) };
    let mut writer = std::fs::File::from(duplicate);
    writer
        .write_all(&frame(EXECUTED_TAG, success_record(manifest, preparation)))
        .and_then(|()| writer.flush())
        .map_err(|_| status_error("test execution acknowledgement could not be published"))
}

fn frame(tag: u8, digest: Sha256Digest) -> [u8; FRAME_BYTES] {
    let mut frame = [0_u8; FRAME_BYTES];
    frame[..4].copy_from_slice(
        &u32::try_from(FRAME_PAYLOAD_BYTES)
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    frame[4] = tag;
    frame[5..].copy_from_slice(digest.as_bytes());
    frame
}

fn decode_frame(
    frame: [u8; FRAME_BYTES],
    manifest: Sha256Digest,
    preparation: Sha256Digest,
) -> FrameValidation {
    let Ok(length_bytes) = frame[..4].try_into() else {
        return FrameValidation::Rejected(
            "helper execution acknowledgement length is malformed",
        );
    };
    let Ok(digest_bytes) = frame[5..].try_into() else {
        return FrameValidation::Rejected(
            "helper execution acknowledgement digest is malformed",
        );
    };
    let length = u32::from_le_bytes(length_bytes);
    let digest = Sha256Digest::new(digest_bytes);
    if usize::try_from(length).ok() != Some(FRAME_PAYLOAD_BYTES) {
        return FrameValidation::Rejected(
            "helper execution acknowledgement length is invalid",
        );
    }
    match frame[4] {
        EXECUTED_TAG if digest == success_record(manifest, preparation) => {
            FrameValidation::Executed
        }
        EXEC_FAILED_TAG if digest == failure_record(manifest, preparation) => {
            FrameValidation::Rejected("native helper could not exec the literal target")
        }
        _ => FrameValidation::Rejected(
            "helper execution acknowledgement is malformed",
        ),
    }
}

fn success_record(manifest: Sha256Digest, preparation: Sha256Digest) -> Sha256Digest {
    peritus_process::native_target_started_record(manifest, preparation)
}

fn failure_record(manifest: Sha256Digest, preparation: Sha256Digest) -> Sha256Digest {
    peritus_process::native_target_exec_failed_record(manifest, preparation)
}

fn status_cancelled() -> MacosError {
    MacosError::new(
        MacosErrorKind::SupervisorFailure,
        MacosOperation::Cancel,
        RecoveryAction::CancelAndReap,
        "helper execution acknowledgement was cancelled by its durable owner",
    )
}

fn status_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Activate,
        RecoveryAction::CancelAndReap,
        detail,
    )
}
