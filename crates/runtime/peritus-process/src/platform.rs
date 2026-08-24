//! Narrow operating-system launch adapters.

mod inheritance;
mod ownership;
mod pipe;
mod pty;
mod resource;

use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};

use peritus_types::Sha256Digest;

use crate::{GracefulAction, NativeProtectedHandle, OutputStream, ProcessError, TerminalSize};

pub(crate) use inheritance::configure_protected_inheritance;
pub use ownership::ProcessTreeIdentity;
pub(crate) use ownership::current_start_token;
pub(crate) use resource::{local_supervisor_resources_supported, sample_resources};

pub(crate) struct OutputReader {
    pub(crate) stream: OutputStream,
    pub(crate) reader: Box<dyn Read + Send>,
}

pub(crate) struct NativeHandshake<'a> {
    pub(crate) manifest: &'a [u8],
    pub(crate) ready: Sha256Digest,
    pub(crate) activated: Sha256Digest,
    #[cfg(windows)]
    pub(crate) started: Sha256Digest,
    pub(crate) protected_handles: &'a [NativeProtectedHandle],
    #[cfg(windows)]
    pub(crate) windows_channels: Option<&'a crate::NativeWindowsHelperChannels>,
}

pub(crate) enum PlatformExit {
    Code(i32),
    #[cfg(unix)]
    Signal(i32),
    #[cfg(unix)]
    SignalName(String),
    PlatformException(u32),
}

pub(crate) trait PlatformProcess: Send {
    fn identity(&self) -> ProcessTreeIdentity;
    fn take_input(&mut self) -> Option<Box<dyn Write + Send>>;
    fn take_readers(&mut self) -> Vec<OutputReader>;
    fn try_wait(&mut self) -> Result<Option<PlatformExit>, ProcessError>;
    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError>;
    fn force_kill(&mut self) -> Result<(), ProcessError>;
    fn tree_quiescent(&mut self) -> Result<bool, ProcessError>;
    fn process_count(&mut self) -> Result<Option<u64>, ProcessError>;
    fn resize(&mut self, size: TerminalSize) -> Result<(), ProcessError>;
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

#[cfg(target_os = "linux")]
pub(crate) fn process_group_count(
    identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    sample_resources(identity).map(|sample| Some(sample.process_count()))
}

#[cfg(all(unix, not(target_os = "linux")))]
pub(crate) const fn process_group_count(
    _identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    Ok(None)
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
    plan: &crate::ExecutionPlan,
    command: &crate::CommandSpec,
    handshake: Option<NativeHandshake<'_>>,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    #[cfg(windows)]
    if matches!(plan.io_mode(), crate::IoMode::Pty(_))
        && handshake.as_ref().and_then(|value| value.windows_channels).is_some()
    {
        return pipe::launch(plan, command, handshake);
    }
    match plan.io_mode() {
        crate::IoMode::Pipes => pipe::launch(plan, command, handshake),
        crate::IoMode::Pty(size) => pty::launch(plan, command, handshake, size),
    }
}

pub(crate) fn verify_helper_record<F>(
    reader: Box<dyn Read + Send>,
    expected: Sha256Digest,
    terminate: F,
) -> Result<Box<dyn Read + Send>, ProcessError>
where
    F: FnOnce(),
{
    let (sender, receiver) = mpsc::sync_channel(1);
    let task = thread::Builder::new()
        .name("peritus-native-handshake".to_owned())
        .spawn(move || {
            let mut reader = reader;
            let mut record = [0_u8; Sha256Digest::LENGTH];
            let result = reader.read_exact(&mut record).map(|()| record);
            let _ = sender.send((reader, result));
        })
        .map_err(|_| helper_protocol_error("native helper handshake task cannot be started"))?;
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok((reader, Ok(record))) => {
            task.join()
                .map_err(|_| helper_protocol_error("native helper handshake task panicked"))?;
            if record != expected.into_bytes() {
                return Err(helper_protocol_error("native helper handshake record mismatched"));
            }
            Ok(reader)
        }
        Ok((_reader, Err(_))) => {
            let _ = task.join();
            Err(helper_protocol_error("native helper handshake stream closed"))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate();
            drop(task);
            Err(helper_protocol_error("native helper handshake timed out"))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = task.join();
            Err(helper_protocol_error("native helper handshake task disconnected"))
        }
    }
}

pub(crate) fn write_helper_manifest(
    writer: &mut dyn Write,
    manifest: &[u8],
) -> Result<(), ProcessError> {
    let length = u32::try_from(manifest.len())
        .map_err(|_| helper_protocol_error("native helper manifest exceeds framing capacity"))?;
    writer
        .write_all(&length.to_le_bytes())
        .and_then(|()| writer.write_all(manifest))
        .and_then(|()| writer.flush())
        .map_err(|_| helper_protocol_error("native helper manifest cannot be delivered"))
}

const fn helper_protocol_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Spawn,
        crate::ProcessOperation::Spawn,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}
