//! Direct structured PTY process launch and session ownership.

#[cfg(unix)]
use std::io::Write;

#[cfg(unix)]
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
#[cfg(unix)]
use portable_pty::{Child, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};

use crate::{
    ErrorCode, ExecutionPlan, ProcessError, ProcessOperation, RecoveryClass, TerminalSize,
};
#[cfg(unix)]
use crate::{GracefulAction, OutputStream, StdinPolicy};

use super::PlatformProcess;
#[cfg(unix)]
use super::{OutputReader, PlatformExit, ProcessTreeIdentity, current_start_token};

#[cfg(windows)]
pub(super) fn launch(
    _plan: &ExecutionPlan,
    _size: TerminalSize,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    Err(ProcessError::new(
        ErrorCode::Unsupported,
        ProcessOperation::Spawn,
        RecoveryClass::SelectBackend,
        "C2 local Windows PTY cannot provide complete descendant job containment",
    ))
}

#[cfg(unix)]
pub(super) fn launch(
    plan: &ExecutionPlan,
    size: TerminalSize,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    let pty_system = NativePtySystem::default();
    let pair =
        pty_system.openpty(to_pty_size(size)).map_err(|_| pty_error("PTY allocation failed"))?;
    let reader =
        pair.master.try_clone_reader().map_err(|_| pty_error("PTY reader cannot be cloned"))?;
    let input = match plan.stdin_policy() {
        StdinPolicy::Closed => None,
        StdinPolicy::Bounded { .. } => {
            Some(pair.master.take_writer().map_err(|_| pty_error("PTY writer cannot be opened"))?)
        }
    };
    let mut command = CommandBuilder::new(plan.command().executable());
    command.args(plan.command().arguments());
    command.cwd(plan.working_directory().path());
    command.env_clear();
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    let child =
        pair.slave.spawn_command(command).map_err(|_| pty_error("PTY child creation failed"))?;
    let root_pid =
        child.process_id().ok_or_else(|| pty_error("PTY child has no process identity"))?;
    let process_group = pair
        .master
        .process_group_leader()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| pty_error("PTY session has no process-group identity"))?;
    let identity = ProcessTreeIdentity::new(
        root_pid,
        current_start_token(root_pid),
        Some(process_group),
        true,
    );
    Ok(Box::new(PtyProcess {
        child,
        master: pair.master,
        identity,
        input,
        readers: vec![OutputReader { stream: OutputStream::Terminal, reader }],
    }))
}

#[cfg(unix)]
struct PtyProcess {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    identity: ProcessTreeIdentity,
    input: Option<Box<dyn Write + Send>>,
    readers: Vec<OutputReader>,
}

#[cfg(unix)]
impl PlatformProcess for PtyProcess {
    fn identity(&self) -> ProcessTreeIdentity {
        self.identity
    }
    fn take_input(&mut self) -> Option<Box<dyn Write + Send>> {
        self.input.take()
    }
    fn take_readers(&mut self) -> Vec<OutputReader> {
        std::mem::take(&mut self.readers)
    }

    fn try_wait(&mut self) -> Result<Option<PlatformExit>, ProcessError> {
        let status = self.child.try_wait().map_err(|_| tree_error("PTY process wait failed"))?;
        Ok(status.map(|status| {
            status.signal().map_or_else(
                || PlatformExit::Code(i32::try_from(status.exit_code()).unwrap_or(i32::MAX)),
                |signal| PlatformExit::SignalName(signal.to_owned()),
            )
        }))
    }

    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError> {
        self.input.take();
        match action {
            GracefulAction::CloseInput => Ok(()),
            GracefulAction::Interrupt => signal_group(self.identity, Signal::SIGINT),
            GracefulAction::Terminate => signal_group(self.identity, Signal::SIGTERM),
        }
    }

    fn force_kill(&mut self) -> Result<(), ProcessError> {
        self.input.take();
        signal_group(self.identity, Signal::SIGKILL)?;
        let _ = self.child.kill();
        Ok(())
    }

    fn tree_quiescent(&mut self) -> Result<bool, ProcessError> {
        super::process_group_quiescent(self.identity)
    }

    fn process_count(&mut self) -> Result<Option<u64>, ProcessError> {
        super::process_group_count(self.identity)
    }

    fn resize(&mut self, size: TerminalSize) -> Result<(), ProcessError> {
        self.master.resize(to_pty_size(size)).map_err(|_| pty_error("PTY resize failed"))
    }
}

#[cfg(unix)]
fn signal_group(identity: ProcessTreeIdentity, signal: Signal) -> Result<(), ProcessError> {
    let group = identity
        .process_group()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| tree_error("PTY process-group identity is unavailable"))?;
    killpg(Pid::from_raw(group), signal).map_err(|_| tree_error("PTY process-group signal failed"))
}

#[cfg(unix)]
const fn to_pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows(),
        cols: size.columns(),
        pixel_width: size.pixel_width(),
        pixel_height: size.pixel_height(),
    }
}

#[cfg(unix)]
const fn pty_error(detail: &'static str) -> ProcessError {
    ProcessError::new(ErrorCode::Pty, ProcessOperation::Spawn, RecoveryClass::SelectBackend, detail)
}

#[cfg(unix)]
const fn tree_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ProcessTree,
        ProcessOperation::Control,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
