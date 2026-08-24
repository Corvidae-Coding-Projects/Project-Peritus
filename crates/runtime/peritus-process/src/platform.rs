//! Narrow operating-system launch adapters.

mod ownership;
mod pipe;
mod pty;
mod resource;

use std::io::{Read, Write};

use crate::{GracefulAction, OutputStream, ProcessError, TerminalSize};

pub use ownership::ProcessTreeIdentity;
pub(crate) use ownership::current_start_token;
pub(crate) use resource::{local_supervisor_resources_supported, sample_resources};

pub(crate) struct OutputReader {
    pub(crate) stream: OutputStream,
    pub(crate) reader: Box<dyn Read + Send>,
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
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    match plan.io_mode() {
        crate::IoMode::Pipes => pipe::launch(plan),
        crate::IoMode::Pty(size) => pty::launch(plan, size),
    }
}
