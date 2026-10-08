//! Narrow operating-system launch adapters.

mod admission;
mod inheritance;
mod ownership;
mod pipe;
mod pty;
mod resource;
#[cfg(windows)]
mod self_memory;
#[cfg(target_os = "linux")]
mod watchdog;

use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::RawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use peritus_types::Sha256Digest;

use crate::{GracefulAction, NativeProtectedHandle, OutputStream, ProcessError, TerminalSize};

#[cfg(target_os = "linux")]
pub(crate) use inheritance::configure_parent_death;
pub(crate) use inheritance::configure_protected_inheritance;
pub(crate) use admission::validate as validate_native_admission;
pub use ownership::ProcessTreeIdentity;
pub(crate) use ownership::current_start_token;
#[cfg(unix)]
pub(crate) use resource::process_group_count;
pub(crate) use resource::{
    local_resource_sampling_available, local_supervisor_resources_supported, sample_resources,
};
#[cfg(windows)]
pub use self_memory::current_process_resident_memory_bytes;

pub(crate) struct OutputReader {
    pub(crate) stream: OutputStream,
    pub(crate) reader: Box<dyn Read + Send>,
}

pub(crate) struct ProcessInput {
    writer: Box<dyn Write + Send>,
    close: InputClose,
}

enum InputClose {
    Pipe,
    #[cfg(unix)]
    Pty(RawFd),
}

pub(crate) struct InputClosePlan {
    frame: Vec<u8>,
    platform_close: bool,
}

impl InputClosePlan {
    fn pipe() -> Self {
        Self { frame: Vec::new(), platform_close: true }
    }

    #[cfg(unix)]
    fn pty(frame: Vec<u8>) -> Self {
        Self { frame, platform_close: false }
    }

    pub(crate) fn into_parts(self) -> (Vec<u8>, bool) {
        (self.frame, self.platform_close)
    }
}

impl ProcessInput {
    pub(crate) fn pipe(writer: Box<dyn Write + Send>) -> Self {
        Self { writer, close: InputClose::Pipe }
    }

    #[cfg(unix)]
    pub(crate) fn pty(writer: Box<dyn Write + Send>, fd: RawFd) -> Self {
        Self { writer, close: InputClose::Pty(fd) }
    }

    pub(crate) fn close_plan(&self) -> Result<InputClosePlan, ProcessError> {
        match &self.close {
            InputClose::Pipe => Ok(InputClosePlan::pipe()),
            #[cfg(unix)]
            InputClose::Pty(fd) => pty_close_plan(*fd),
        }
    }
}

impl Write for ProcessInput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

pub(crate) struct NativeHandshake {
    pub(crate) manifest: Vec<Vec<u8>>,
    pub(crate) ready: Sha256Digest,
    pub(crate) activated: Sha256Digest,
    #[cfg(windows)]
    pub(crate) started: Sha256Digest,
    #[cfg(windows)]
    pub(crate) quiesced: Sha256Digest,
    #[cfg(windows)]
    pub(crate) worker_failed: Sha256Digest,
    #[cfg(windows)]
    pub(crate) completion: Sha256Digest,
    #[cfg(windows)]
    pub(crate) adoption: Option<Sha256Digest>,
    pub(crate) protected_handles: Vec<NativeProtectedHandle>,
    #[cfg(windows)]
    pub(crate) windows_channels: Option<crate::NativeWindowsHelperChannels>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeHandshakeStatus {
    Complete,
    Cancelled,
    Failed,
}

pub(crate) struct PlatformLaunch {
    process: Box<dyn PlatformProcess>,
    handshake: NativeHandshakeStatus,
}

impl PlatformLaunch {
    pub(crate) const fn new(
        process: Box<dyn PlatformProcess>,
        handshake: NativeHandshakeStatus,
    ) -> Self {
        Self { process, handshake }
    }

    pub(crate) fn into_parts(self) -> (Box<dyn PlatformProcess>, NativeHandshakeStatus) {
        (self.process, self.handshake)
    }
}

pub(crate) enum HandshakeError {
    Cancelled,
    Failed(ProcessError),
}

impl HandshakeError {
    pub(crate) const fn status(self) -> NativeHandshakeStatus {
        match self {
            Self::Cancelled => NativeHandshakeStatus::Cancelled,
            Self::Failed(_) => NativeHandshakeStatus::Failed,
        }
    }
}

pub(crate) enum PlatformExit {
    Code(i32),
    #[cfg(unix)]
    Signal(i32),
    #[cfg(unix)]
    SignalName(String),
    PlatformException(u32),
    NativeFailure(crate::NativeFailureObservation),
    Unavailable,
}

pub(crate) trait PlatformProcess: Send {
    fn identity(&self) -> ProcessTreeIdentity;
    fn take_input(&mut self) -> Option<ProcessInput>;
    fn take_readers(&mut self) -> Vec<OutputReader>;
    fn try_wait(&mut self) -> Result<Option<PlatformExit>, ProcessError>;
    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError>;
    fn force_kill(&mut self) -> Result<(), ProcessError>;
    fn tree_quiescent(&mut self) -> Result<bool, ProcessError>;
    fn process_count(&mut self) -> Result<Option<u64>, ProcessError>;
    fn resize(&mut self, size: TerminalSize) -> Result<(), ProcessError>;
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "fcntl is the narrow Unix boundary for making the owned stdin descriptor cancellable"
)]
pub(crate) fn set_input_nonblocking(fd: RawFd) -> Result<(), ProcessError> {
    // SAFETY: `fd` remains owned by the caller for both fcntl operations.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(input_control_error("process input flags cannot be read"));
    }
    // SAFETY: the descriptor remains live and F_SETFL updates only its status flags.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(input_control_error("process input cannot be made nonblocking"));
    }
    Ok(())
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "tcgetattr and fpathconf query the live PTY input contract at ordered close execution"
)]
fn pty_close_plan(fd: RawFd) -> Result<InputClosePlan, ProcessError> {
    let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: `attributes` points to writable storage and ProcessInput retains the live descriptor.
    if unsafe { libc::tcgetattr(fd, attributes.as_mut_ptr()) } != 0 {
        return Err(input_close_error("current PTY input settings cannot be read"));
    }
    // SAFETY: tcgetattr initialized the full termios value after returning success.
    let attributes = unsafe { attributes.assume_init() };
    if attributes.c_lflag & libc::ICANON == 0 {
        return Ok(InputClosePlan::pty(Vec::new()));
    }
    // SAFETY: fd remains live and _PC_VDISABLE only queries this terminal's control-character
    // convention; a negative result cannot establish truthful VEOF semantics.
    let disabled = unsafe { libc::fpathconf(fd, libc::_PC_VDISABLE) };
    if disabled < 0 {
        return Err(input_close_error("PTY disabled-control value cannot be established"));
    }
    let eof = attributes.c_cc[libc::VEOF];
    if eof as libc::c_long == disabled {
        return Ok(InputClosePlan::pty(Vec::new()));
    }
    Ok(InputClosePlan::pty(vec![b'\n', eof]))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "CancelSynchronousIo is the Windows boundary for waking the uniquely owned stdin task"
)]
pub(crate) fn cancel_input_write(
    task: &std::thread::JoinHandle<()>,
) -> Result<(), ProcessError> {
    use windows_sys::Win32::{
        Foundation::ERROR_NOT_FOUND,
        System::IO::CancelSynchronousIo,
    };

    // SAFETY: the retained JoinHandle owns the exact input thread handle for the full call.
    if unsafe { CancelSynchronousIo(task.as_raw_handle()) } != 0 {
        return Ok(());
    }
    if std::io::Error::last_os_error()
        .raw_os_error()
        .and_then(|code| u32::try_from(code).ok())
        == Some(ERROR_NOT_FOUND)
    {
        return Ok(());
    }
    Err(input_control_error("blocked Windows process input cannot be cancelled"))
}

#[cfg(not(windows))]
pub(crate) const fn cancel_input_write(
    _task: &std::thread::JoinHandle<()>,
) -> Result<(), ProcessError> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn process_group_quiescent(identity: ProcessTreeIdentity) -> Result<bool, ProcessError> {
    use nix::{errno::Errno, sys::signal::kill, unistd::Pid};

    let group = identity
        .process_group()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| tree_observation_error("process-group identity is unavailable"))?;
    match kill(Pid::from_raw(-group), None) {
        Ok(()) | Err(Errno::EPERM) => Ok(false),
        Err(Errno::ESRCH) => Ok(true),
        Err(_) => Err(tree_observation_error("process-group quiescence cannot be observed")),
    }
}

#[cfg(unix)]
const fn tree_observation_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::ProcessTree,
        crate::ProcessOperation::Wait,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}

pub(crate) fn launch(
    plan: &crate::supervisor::SupervisorPlan,
    command: &crate::CommandSpec,
    handshake: Option<NativeHandshake>,
    crash_watchdog: Option<&std::path::Path>,
    spawned: &mut dyn FnMut(ProcessTreeIdentity) -> Result<(), ProcessError>,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<PlatformLaunch, ProcessError> {
    let kill_on_parent_death = crash_watchdog.is_none();
    #[cfg(windows)]
    if matches!(plan.io_mode(), crate::IoMode::Pty(_))
        && handshake
            .as_ref()
            .and_then(|value| value.windows_channels.as_ref())
            .is_some()
    {
        return pipe::launch(
            plan,
            command,
            handshake,
            kill_on_parent_death,
            spawned,
            should_continue,
        );
    }
    let launch = match plan.io_mode() {
        crate::IoMode::Pipes => pipe::launch(
            plan,
            command,
            handshake,
            kill_on_parent_death,
            spawned,
            should_continue,
        ),
        crate::IoMode::Pty(size) => pty::launch(
            plan,
            command,
            handshake,
            size,
            kill_on_parent_death,
            spawned,
            should_continue,
        ),
    }?;
    #[cfg(target_os = "linux")]
    if let Some(executable) = crash_watchdog {
        let (process, handshake) = launch.into_parts();
        return watchdog::attach(process, executable)
            .map(|process| PlatformLaunch::new(process, handshake));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = crash_watchdog;
    Ok(launch)
}

pub(crate) fn verify_helper_record(
    mut reader: Box<dyn Read + Send>,
    expected: Sha256Digest,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<Box<dyn Read + Send>, HandshakeError> {
    use std::io::ErrorKind;

    let mut record = [0_u8; Sha256Digest::LENGTH];
    let mut offset = 0;
    while offset < record.len() {
        if !should_continue() {
            return Err(HandshakeError::Cancelled);
        }
        match reader.read(&mut record[offset..]) {
            Ok(0) => {
                return Err(HandshakeError::Failed(helper_protocol_error(
                    "native helper handshake stream closed",
                )));
            }
            Ok(count) => offset += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(_) => {
                return Err(HandshakeError::Failed(helper_protocol_error(
                    "native helper handshake stream could not be read",
                )));
            }
        }
    }
    if record != expected.into_bytes() {
        return Err(HandshakeError::Failed(helper_protocol_error(
            "native helper handshake record mismatched",
        )));
    }
    Ok(reader)
}

pub(crate) fn write_helper_manifest(
    writer: &mut dyn Write,
    manifest: &[Vec<u8>],
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), HandshakeError> {
    if manifest.is_empty() {
        return Err(HandshakeError::Failed(helper_protocol_error(
            "native helper manifest page stream is empty",
        )));
    }
    if manifest.len() == 1 {
        write_manifest_page(writer, &manifest[0], should_continue)?;
        return flush_while(writer, should_continue);
    }
    write_all_while(
        writer,
        &crate::NATIVE_MANIFEST_STREAM_MARKER.to_le_bytes(),
        should_continue,
    )?;
    for page in manifest {
        write_manifest_page(writer, page, should_continue)?;
    }
    write_all_while(writer, &0_u32.to_le_bytes(), should_continue)?;
    flush_while(writer, should_continue)
}

fn write_manifest_page(
    writer: &mut dyn Write,
    page: &[u8],
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), HandshakeError> {
    if page.is_empty() || page.len() > crate::NATIVE_MANIFEST_FRAME_BYTES {
        return Err(HandshakeError::Failed(helper_protocol_error(
            "native helper manifest page exceeds physical transport capacity",
        )));
    }
    let length = u32::try_from(page.len()).map_err(|_| {
        HandshakeError::Failed(helper_protocol_error(
            "native helper manifest page length is not representable",
        ))
    })?;
    write_all_while(writer, &length.to_le_bytes(), should_continue)?;
    write_all_while(writer, page, should_continue)
}

fn write_all_while(
    writer: &mut dyn Write,
    bytes: &[u8],
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), HandshakeError> {
    use std::io::ErrorKind;

    let mut offset = 0;
    while offset < bytes.len() {
        if !should_continue() {
            return Err(HandshakeError::Cancelled);
        }
        match writer.write(&bytes[offset..]) {
            Ok(0) => {
                return Err(HandshakeError::Failed(helper_protocol_error(
                    "native helper manifest channel closed",
                )));
            }
            Ok(count) => offset += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(_) => {
                return Err(HandshakeError::Failed(helper_protocol_error(
                    "native helper manifest cannot be delivered",
                )));
            }
        }
    }
    Ok(())
}

fn flush_while(
    writer: &mut dyn Write,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), HandshakeError> {
    use std::io::ErrorKind;

    loop {
        if !should_continue() {
            return Err(HandshakeError::Cancelled);
        }
        match writer.flush() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(_) => {
                return Err(HandshakeError::Failed(helper_protocol_error(
                    "native helper manifest cannot be delivered",
                )));
            }
        }
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "fcntl is the narrow Unix boundary for cancellable native helper protocol I/O"
)]
pub(crate) fn set_protocol_nonblocking(fd: RawFd) -> Result<(), ProcessError> {
    // SAFETY: `fd` remains owned by the caller and these operations only change its status flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(helper_protocol_error(
            "native helper protocol descriptor cannot be made nonblocking",
        ));
    }
    Ok(())
}

const fn helper_protocol_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Spawn,
        crate::ProcessOperation::Spawn,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}

const fn input_control_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Input,
        crate::ProcessOperation::Stream,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}

const fn input_close_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Input,
        crate::ProcessOperation::Stream,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}
