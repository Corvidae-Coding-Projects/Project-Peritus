//! Direct structured PTY process launch and session ownership.

#[cfg(unix)]
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[cfg(unix)]
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
#[cfg(unix)]
use portable_pty::{Child, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
#[cfg(unix)]
use process_wrap::std::{ChildWrapper, CommandWrap, ProcessSession};

use crate::{
    CommandSpec, ErrorCode, ExecutionPlan, ProcessError, ProcessOperation, RecoveryClass,
    TerminalSize,
};
#[cfg(unix)]
use crate::{GracefulAction, OutputStream, StdinPolicy};

use super::PlatformProcess;
#[cfg(unix)]
use super::{
    NativeHandshake, OutputReader, PlatformExit, ProcessTreeIdentity, current_start_token,
};

#[cfg(windows)]
pub(super) fn launch(
    _plan: &ExecutionPlan,
    _command: &CommandSpec,
    _handshake: Option<super::NativeHandshake<'_>>,
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
    launch_command: &CommandSpec,
    handshake: Option<NativeHandshake<'_>>,
    size: TerminalSize,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    let pty_system = NativePtySystem::default();
    let pair =
        pty_system.openpty(to_pty_size(size)).map_err(|_| pty_error("PTY allocation failed"))?;
    let reader =
        pair.master.try_clone_reader().map_err(|_| pty_error("PTY reader cannot be cloned"))?;
    let needs_writer = matches!(plan.stdin_policy(), StdinPolicy::Bounded { .. });
    let input = if needs_writer {
        Some(pair.master.take_writer().map_err(|_| pty_error("PTY writer cannot be opened"))?)
    } else {
        None
    };
    if let Some(handshake) = handshake {
        return launch_native(plan, launch_command, &handshake, pair, reader, input);
    }
    let mut command = CommandBuilder::new(launch_command.executable());
    command.args(launch_command.arguments());
    command.cwd(plan.working_directory().path());
    command.env_clear();
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    let child =
        pair.slave.spawn_command(command).map_err(|_| pty_error("PTY child creation failed"))?;
    let reader: Box<dyn std::io::Read + Send> = Box::new(reader);
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
        child: PtyChild::Portable(child),
        master: pair.master,
        identity,
        input,
        readers: vec![OutputReader { stream: OutputStream::Terminal, reader }],
    }))
}

#[cfg(unix)]
fn launch_native(
    plan: &ExecutionPlan,
    launch_command: &CommandSpec,
    handshake: &NativeHandshake<'_>,
    pair: portable_pty::PtyPair,
    terminal_reader: Box<dyn std::io::Read + Send>,
    input: Option<Box<dyn Write + Send>>,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    let slave_path = pair
        .master
        .tty_name()
        .ok_or_else(|| pty_error("native PTY slave identity is unavailable"))?;
    let mut command = Command::new(launch_command.executable());
    command
        .args(launch_command.arguments())
        .current_dir(plan.working_directory().path())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    command.env(crate::NATIVE_PTY_SLAVE_ENV, slave_path);
    let child = {
        let _inheritance =
            super::configure_protected_inheritance(&mut command, handshake.protected_handles)?;
        let mut wrapped = CommandWrap::from(command);
        wrapped.wrap(ProcessSession);
        wrapped.spawn()
    };
    let mut child = child.map_err(|_| pty_error("native PTY helper creation failed"))?;
    let root_pid = child.id();
    let mut protocol_input =
        child.stdin().take().ok_or_else(|| pty_error("native PTY helper has no protocol input"))?;
    let protocol_output = child
        .stdout()
        .take()
        .map(|reader| Box::new(reader) as Box<dyn std::io::Read + Send>)
        .ok_or_else(|| pty_error("native PTY helper has no protocol output"))?;
    let protocol_output =
        match super::verify_helper_record(protocol_output, handshake.ready, || {
            let _ = child.start_kill();
        }) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.start_kill();
                return Err(error);
            }
        };
    super::write_helper_manifest(&mut protocol_input, handshake.manifest)?;
    let protocol_output =
        match super::verify_helper_record(protocol_output, handshake.activated, || {
            let _ = child.start_kill();
        }) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.start_kill();
                return Err(error);
            }
        };
    drop(protocol_input);
    drop(protocol_output);
    drop(pair.slave);
    let identity =
        ProcessTreeIdentity::new(root_pid, current_start_token(root_pid), Some(root_pid), true);
    Ok(Box::new(PtyProcess {
        child: PtyChild::Native(child),
        master: pair.master,
        identity,
        input,
        readers: vec![OutputReader { stream: OutputStream::Terminal, reader: terminal_reader }],
    }))
}

#[cfg(unix)]
struct PtyProcess {
    child: PtyChild,
    master: Box<dyn MasterPty + Send>,
    identity: ProcessTreeIdentity,
    input: Option<Box<dyn Write + Send>>,
    readers: Vec<OutputReader>,
}

#[cfg(unix)]
enum PtyChild {
    Portable(Box<dyn Child + Send + Sync>),
    Native(Box<dyn ChildWrapper>),
}

#[cfg(unix)]
impl PtyChild {
    fn try_wait(&mut self) -> std::io::Result<Option<PlatformExit>> {
        match self {
            Self::Portable(child) => child.try_wait().map(|status| {
                status.map(|status| {
                    status.signal().map_or_else(
                        || {
                            PlatformExit::Code(
                                i32::try_from(status.exit_code()).unwrap_or(i32::MAX),
                            )
                        },
                        |signal| PlatformExit::SignalName(signal.to_owned()),
                    )
                })
            }),
            Self::Native(child) => child.try_wait().map(|status| status.map(convert_native_status)),
        }
    }

    fn kill(&mut self) {
        match self {
            Self::Portable(child) => {
                let _ = child.kill();
            }
            Self::Native(child) => {
                let _ = child.start_kill();
            }
        }
    }
}

#[cfg(unix)]
fn convert_native_status(status: std::process::ExitStatus) -> PlatformExit {
    use std::os::unix::process::ExitStatusExt;

    status
        .signal()
        .map_or_else(|| PlatformExit::Code(status.code().unwrap_or(i32::MAX)), PlatformExit::Signal)
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
        self.child.try_wait().map_err(|_| tree_error("PTY process wait failed"))
    }

    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError> {
        if action == GracefulAction::CloseInput {
            self.input.take();
        }
        match action {
            GracefulAction::CloseInput => Ok(()),
            GracefulAction::Interrupt => signal_group(self.identity, Signal::SIGINT),
            GracefulAction::Terminate => signal_group(self.identity, Signal::SIGTERM),
        }
    }

    fn force_kill(&mut self) -> Result<(), ProcessError> {
        self.input.take();
        signal_group(self.identity, Signal::SIGKILL)?;
        self.child.kill();
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
