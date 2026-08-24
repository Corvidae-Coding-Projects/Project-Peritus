//! Protected Windows helper status and terminal-control channels.

#![allow(
    unsafe_code,
    reason = "CreatePipe and inherited HANDLE ownership are the narrow Windows channel boundary"
)]

use std::{
    fs::File,
    io::Write,
    os::windows::io::{FromRawHandle, RawHandle},
    sync::{Arc, Mutex},
};

use windows_sys::Win32::{Foundation::HANDLE, System::Pipes::CreatePipe};

use crate::{
    ErrorCode, NativeProtectedHandle, ProcessError, ProcessOperation, RecoveryClass, TerminalSize,
};

/// Reserved child environment key carrying the digest-bound started-status writer.
pub const NATIVE_WINDOWS_STATUS_HANDLE_ENV: &str = "PERITUS_NATIVE_WINDOWS_STATUS_V1";
/// Reserved child environment key carrying the terminal resize-control reader.
pub const NATIVE_WINDOWS_CONTROL_HANDLE_ENV: &str = "PERITUS_NATIVE_WINDOWS_CONTROL_V1";

/// C2-owned parent endpoints and their exact protected helper endpoints.
#[derive(Clone, Debug)]
pub struct NativeWindowsHelperChannels {
    status_reader: Arc<File>,
    control_writer: Arc<Mutex<File>>,
    child_handles: Vec<NativeProtectedHandle>,
    status_handle: u64,
    control_handle: u64,
}

impl NativeWindowsHelperChannels {
    /// Creates one status pipe and one resize-control pipe.
    ///
    /// # Errors
    /// Returns a typed spawn failure if either anonymous pipe cannot be created.
    pub fn new() -> Result<Self, ProcessError> {
        let (status_reader, status_writer) = pipe()?;
        let (control_reader, control_writer) = pipe()?;
        let status = NativeProtectedHandle::from_file("windows-helper-status-v1", status_writer)?;
        let control =
            NativeProtectedHandle::from_file("windows-terminal-control-v1", control_reader)?;
        let status_handle = status.raw_handle();
        let control_handle = control.raw_handle();
        Ok(Self {
            status_reader: Arc::new(status_reader),
            control_writer: Arc::new(Mutex::new(control_writer)),
            child_handles: vec![status, control],
            status_handle,
            control_handle,
        })
    }

    pub(crate) fn take_child_handles(&mut self) -> Vec<NativeProtectedHandle> {
        core::mem::take(&mut self.child_handles)
    }

    pub(crate) const fn status_handle(&self) -> u64 {
        self.status_handle
    }

    pub(crate) const fn control_handle(&self) -> u64 {
        self.control_handle
    }

    pub(crate) fn status_reader(&self) -> Result<File, ProcessError> {
        self.status_reader
            .try_clone()
            .map_err(|_| channel_error("Windows helper status reader cannot be cloned"))
    }

    pub(crate) fn resize(&self, size: TerminalSize) -> Result<(), ProcessError> {
        let mut frame = [0_u8; 5];
        frame[0] = 1;
        frame[1..3].copy_from_slice(&size.columns().to_le_bytes());
        frame[3..5].copy_from_slice(&size.rows().to_le_bytes());
        self.write_control(&frame)
    }

    pub(crate) fn graceful(&self, action: crate::GracefulAction) -> Result<(), ProcessError> {
        let tag = match action {
            crate::GracefulAction::Interrupt => 2,
            crate::GracefulAction::Terminate => 3,
            crate::GracefulAction::CloseInput => 4,
        };
        self.write_control(&[tag])
    }

    fn write_control(&self, frame: &[u8]) -> Result<(), ProcessError> {
        let mut writer = self
            .control_writer
            .lock()
            .map_err(|_| channel_error("Windows terminal control channel was poisoned"))?;
        writer
            .write_all(frame)
            .and_then(|()| writer.flush())
            .map_err(|_| channel_error("Windows terminal resize cannot be delivered"))
    }
}

/// Helper-owned inherited status/control endpoints opened from C2-reserved environment values.
#[derive(Debug)]
pub struct NativeWindowsHelperAttachment {
    status: File,
    control: Option<File>,
}

impl NativeWindowsHelperAttachment {
    /// Opens the exact inherited channel handles.
    ///
    /// # Errors
    /// Rejects missing, malformed, or non-handle environment values.
    pub fn from_environment() -> Result<Self, ProcessError> {
        let status = inherited_file(NATIVE_WINDOWS_STATUS_HANDLE_ENV)?;
        let control = inherited_file(NATIVE_WINDOWS_CONTROL_HANDLE_ENV)?;
        Ok(Self { status, control: Some(control) })
    }

    /// Writes the digest-bound record proving that the target was successfully resumed.
    ///
    /// # Errors
    /// Returns a protocol error if C2 can no longer observe the record.
    pub fn signal_started(&mut self, record: [u8; 32]) -> Result<(), ProcessError> {
        self.status
            .write_all(&record)
            .and_then(|()| self.status.flush())
            .map_err(|_| channel_error("Windows target-started record cannot be written"))
    }

    /// Transfers the resize reader to the `ConPTY` control loop.
    #[must_use]
    pub const fn take_control_reader(&mut self) -> Option<File> {
        self.control.take()
    }
}

fn pipe() -> Result<(File, File), ProcessError> {
    let mut reader: HANDLE = std::ptr::null_mut();
    let mut writer: HANDLE = std::ptr::null_mut();
    // SAFETY: output pointers are valid; null attributes create initially non-inheritable handles.
    if unsafe { CreatePipe(&raw mut reader, &raw mut writer, std::ptr::null(), 0) } == 0 {
        return Err(channel_error("Windows anonymous helper channel cannot be created"));
    }
    // SAFETY: both non-null handles were returned by CreatePipe and ownership moves into File.
    let reader = unsafe { File::from_raw_handle(reader.cast()) };
    // SAFETY: paired writer is independently owned and also moves into File.
    let writer = unsafe { File::from_raw_handle(writer.cast()) };
    Ok((reader, writer))
}

fn inherited_file(key: &'static str) -> Result<File, ProcessError> {
    let value =
        std::env::var_os(key).ok_or_else(|| channel_error("Windows helper channel is missing"))?;
    let text = value
        .to_str()
        .ok_or_else(|| channel_error("Windows helper channel identity is not Unicode"))?;
    let raw = text
        .parse::<usize>()
        .map_err(|_| channel_error("Windows helper channel identity is malformed"))?;
    if raw == 0 || raw == usize::MAX {
        return Err(channel_error("Windows helper channel identity is invalid"));
    }
    let handle = raw as RawHandle;
    // SAFETY: C2 supplies an exact uniquely inherited child HANDLE and removes the environment
    // identity before any target command is created; this attachment assumes its ownership.
    Ok(unsafe { File::from_raw_handle(handle) })
}

const fn channel_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Spawn,
        ProcessOperation::Spawn,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
