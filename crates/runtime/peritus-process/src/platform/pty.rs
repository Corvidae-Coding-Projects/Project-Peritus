//! Direct structured PTY process launch and session ownership.

#[cfg(unix)]
use std::{
    fs::File,
    os::fd::{AsRawFd, FromRawFd},
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
    CommandSpec, ErrorCode, ProcessError, ProcessOperation, RecoveryClass, TerminalSize,
    supervisor::SupervisorPlan,
};
#[cfg(unix)]
use crate::{GracefulAction, OutputStream, StdinPolicy};

use super::PlatformProcess;
#[cfg(unix)]
use super::{
    NativeHandshake, OutputReader, PlatformExit, ProcessInput, ProcessTreeIdentity,
    current_start_token,
};

#[cfg(windows)]
pub(super) fn launch(
    _plan: &SupervisorPlan,
    _command: &CommandSpec,
    _handshake: Option<super::NativeHandshake>,
    _size: TerminalSize,
    _kill_on_parent_death: bool,
    _spawned: &mut dyn FnMut(super::ProcessTreeIdentity) -> Result<(), ProcessError>,
    _should_continue: &mut dyn FnMut() -> bool,
) -> Result<super::PlatformLaunch, ProcessError> {
    Err(ProcessError::new(
        ErrorCode::Unsupported,
        ProcessOperation::Spawn,
        RecoveryClass::SelectBackend,
        "C2 local Windows PTY cannot provide complete descendant job containment",
    ))
}

#[cfg(unix)]
pub(super) fn launch(
    plan: &SupervisorPlan,
    launch_command: &CommandSpec,
    handshake: Option<NativeHandshake>,
    size: TerminalSize,
    kill_on_parent_death: bool,
    spawned: &mut dyn FnMut(ProcessTreeIdentity) -> Result<(), ProcessError>,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<super::PlatformLaunch, ProcessError> {
    let pty_system = NativePtySystem::default();
    let pair =
        pty_system.openpty(to_pty_size(size)).map_err(|_| pty_error("PTY allocation failed"))?;
    let needs_writer = !matches!(plan.stdin_policy(), StdinPolicy::Closed);
    let reader =
        pair.master.try_clone_reader().map_err(|_| pty_error("PTY reader cannot be cloned"))?;
    let input = if needs_writer {
        Some(pty_input(&*pair.master)?)
    } else {
        None
    };
    if let Some(handshake) = handshake {
        return launch_native(
            plan,
            launch_command,
            &handshake,
            pair,
            reader,
            input,
            kill_on_parent_death,
            spawned,
            should_continue,
        );
    }
    let mut command = CommandBuilder::new(launch_command.executable());
    command.args(launch_command.arguments());
    command.cwd(plan.working_directory());
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
    let process = Box::new(PtyProcess {
        child: PtyChild::Portable(child),
        master: pair.master,
        identity,
        input,
        readers: vec![OutputReader { stream: OutputStream::Terminal, reader }],
    });
    Ok(super::PlatformLaunch::new(
        process,
        super::NativeHandshakeStatus::Activated,
    ))
}

#[cfg(unix)]
fn launch_native(
    plan: &SupervisorPlan,
    launch_command: &CommandSpec,
    handshake: &NativeHandshake,
    pair: portable_pty::PtyPair,
    terminal_reader: Box<dyn std::io::Read + Send>,
    input: Option<ProcessInput>,
    kill_on_parent_death: bool,
    spawned: &mut dyn FnMut(ProcessTreeIdentity) -> Result<(), ProcessError>,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<super::PlatformLaunch, ProcessError> {
    let slave_path = pair
        .master
        .tty_name()
        .ok_or_else(|| pty_error("native PTY slave identity is unavailable"))?;
    let mut command = Command::new(launch_command.executable());
    command
        .args(launch_command.arguments())
        .current_dir(plan.working_directory())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    #[cfg(target_os = "linux")]
    if kill_on_parent_death {
        super::configure_parent_death(&mut command);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = kill_on_parent_death;
    command.env(crate::NATIVE_PTY_SLAVE_ENV, slave_path);
    let child = {
        let _inheritance =
            super::configure_protected_inheritance(&mut command, &handshake.protected_handles)?;
        let mut wrapped = CommandWrap::from(command);
        wrapped.wrap(ProcessSession);
        wrapped.spawn()
    };
    let mut child = child.map_err(|_| pty_error("native PTY helper creation failed"))?;
    let root_pid = child.id();
    let identity =
        ProcessTreeIdentity::new(root_pid, current_start_token(root_pid), Some(root_pid), true);
    let mut protocol_input = child.stdin().take();
    let mut protocol_output = child.stdout().take();
    let descriptors_ready = protocol_input
        .as_ref()
        .is_some_and(|writer| super::set_protocol_nonblocking(writer.as_raw_fd()).is_ok())
        && protocol_output
            .as_ref()
            .is_some_and(|reader| super::set_protocol_nonblocking(reader.as_raw_fd()).is_ok());
    let mut handshake_status = if spawned(identity).is_ok() && descriptors_ready {
        super::NativeHandshakeStatus::Prepared
    } else {
        super::NativeHandshakeStatus::Failed
    };
    let mut ready_output = None;
    if matches!(handshake_status, super::NativeHandshakeStatus::Prepared) {
        let ready = (|| {
            let protocol_output = protocol_output
                .take()
                .map(|reader| Box::new(reader) as Box<dyn std::io::Read + Send>)
                .ok_or_else(|| {
                    super::HandshakeError::Failed(pty_error(
                        "native PTY helper has no protocol output",
                    ))
                })?;
            let protocol_output = super::verify_helper_record(
                protocol_output,
                handshake.ready,
                should_continue,
            )?;
            Ok(protocol_output)
        })();
        match ready {
            Ok(output) => {
                handshake_status = super::NativeHandshakeStatus::Ready;
                ready_output = Some(output);
            }
            Err(error) => handshake_status = error.status(),
        }
    }
    if matches!(handshake_status, super::NativeHandshakeStatus::Ready) {
        let activation = (|| {
            let protocol_output = ready_output.take().ok_or_else(|| {
                super::HandshakeError::Failed(pty_error(
                    "native PTY helper ready stream disappeared",
                ))
            })?;
            let protocol_input = protocol_input.as_mut().ok_or_else(|| {
                super::HandshakeError::Failed(pty_error(
                    "native PTY helper has no protocol input",
                ))
            })?;
            super::write_helper_manifest(protocol_input, &handshake.manifest, should_continue)?;
            super::verify_helper_record(
                protocol_output,
                handshake.activated,
                should_continue,
            )
        })();
        match activation {
            Ok(output) => {
                handshake_status = super::NativeHandshakeStatus::Activated;
                drop(output);
            }
            Err(error) => handshake_status = error.status(),
        }
    }
    drop(protocol_input);
    drop(protocol_output);
    drop(pair.slave);
    let process = Box::new(PtyProcess {
        child: PtyChild::Native(child),
        master: pair.master,
        identity,
        input,
        readers: vec![OutputReader { stream: OutputStream::Terminal, reader: terminal_reader }],
    });
    Ok(super::PlatformLaunch::new(process, handshake_status))
}

#[cfg(unix)]
struct PtyProcess {
    child: PtyChild,
    master: Box<dyn MasterPty + Send>,
    identity: ProcessTreeIdentity,
    input: Option<ProcessInput>,
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
    fn take_input(&mut self) -> Option<ProcessInput> {
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
#[allow(
    unsafe_code,
    reason = "the PTY input task needs one close-on-exec duplicate of the owned raw master descriptor"
)]
fn pty_input(master: &dyn MasterPty) -> Result<ProcessInput, ProcessError> {
    let fd = master
        .as_raw_fd()
        .ok_or_else(|| pty_error("PTY input descriptor is unavailable"))?;
    // SAFETY: F_DUPFD_CLOEXEC duplicates the live master descriptor into independent ownership.
    let input_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if input_fd == -1 {
        return Err(pty_error("PTY input descriptor cannot be duplicated"));
    }
    if let Err(error) = super::set_input_nonblocking(input_fd) {
        // SAFETY: `input_fd` is the uniquely owned duplicate returned immediately above.
        unsafe { libc::close(input_fd) };
        return Err(error);
    }
    // SAFETY: ownership of the unique duplicated descriptor moves into File exactly once.
    let writer = unsafe { File::from_raw_fd(input_fd) };
    Ok(ProcessInput::pty(Box::new(writer), input_fd))
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
