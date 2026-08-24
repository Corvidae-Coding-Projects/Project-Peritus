//! Direct structured pipe process launch with process-tree containment.

use std::{
    io::Write,
    process::{Command, Stdio},
};

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessSession;
use process_wrap::std::{ChildWrapper, CommandWrap};
#[cfg(windows)]
use std::{
    sync::mpsc::{Receiver, TryRecvError, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    CommandSpec, ErrorCode, ExecutionPlan, GracefulAction, OutputStream, ProcessError,
    ProcessOperation, RecoveryClass, StdinPolicy, TerminalSize,
};

use super::{
    NativeHandshake, OutputReader, PlatformExit, PlatformProcess, ProcessTreeIdentity,
    current_start_token,
};

#[allow(
    clippy::too_many_lines,
    reason = "the direct-child spawn and bounded native handshake are one rollback transaction"
)]
pub(super) fn launch(
    plan: &ExecutionPlan,
    launch_command: &CommandSpec,
    handshake: Option<NativeHandshake<'_>>,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    #[cfg(windows)]
    let native_windows_pty = matches!(plan.io_mode(), crate::IoMode::Pty(_))
        && handshake.as_ref().and_then(|value| value.windows_channels).is_some();
    #[cfg(not(windows))]
    let native_windows_pty = false;
    let mut command = Command::new(launch_command.executable());
    command
        .args(launch_command.arguments())
        .current_dir(plan.working_directory().path())
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(if native_windows_pty { Stdio::null() } else { Stdio::piped() });
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    #[cfg(windows)]
    if let Some(channels) = handshake.as_ref().and_then(|value| value.windows_channels) {
        command.env(crate::NATIVE_WINDOWS_STATUS_HANDLE_ENV, channels.status_handle().to_string());
        command
            .env(crate::NATIVE_WINDOWS_CONTROL_HANDLE_ENV, channels.control_handle().to_string());
    }
    let protected_handles = handshake.as_ref().map_or(&[][..], |value| value.protected_handles);
    match (handshake.is_some(), plan.stdin_policy()) {
        (false, StdinPolicy::Closed) => {
            command.stdin(Stdio::null());
        }
        (false, StdinPolicy::Bounded { .. }) | (true, _) => {
            command.stdin(Stdio::piped());
        }
    }
    #[cfg(windows)]
    let windows_channels = handshake.as_ref().and_then(|value| value.windows_channels).cloned();
    #[cfg(windows)]
    let status_reader = windows_channels
        .as_ref()
        .map(crate::NativeWindowsHelperChannels::status_reader)
        .transpose()?;
    let child = {
        let _inheritance = super::configure_protected_inheritance(&mut command, protected_handles)?;
        let mut wrapped = CommandWrap::from(command);
        #[cfg(unix)]
        wrapped.wrap(ProcessSession);
        #[cfg(windows)]
        wrapped.wrap(JobObject);
        wrapped.spawn()
    };
    let mut child = child.map_err(|_| spawn_error("pipe process creation failed"))?;
    let root_pid = child.id();
    let mut input = child.stdin().take().map(|input| Box::new(input) as Box<dyn Write + Send>);
    let stdout =
        child.stdout().take().map(|reader| Box::new(reader) as Box<dyn std::io::Read + Send>);
    let stdout = if let Some(handshake) = handshake {
        let reader =
            stdout.ok_or_else(|| spawn_error("native helper has no activation output stream"))?;
        let reader = match super::verify_helper_record(reader, handshake.ready, || {
            let _ = child.start_kill();
        }) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.start_kill();
                return Err(error);
            }
        };
        let writer = input
            .as_deref_mut()
            .ok_or_else(|| spawn_error("native helper has no manifest input stream"))?;
        super::write_helper_manifest(writer, handshake.manifest)?;
        let reader = match super::verify_helper_record(reader, handshake.activated, || {
            let _ = child.start_kill();
        }) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.start_kill();
                return Err(error);
            }
        };
        #[cfg(windows)]
        if let Some(status_reader) = status_reader {
            let _status = match super::verify_helper_record(
                Box::new(status_reader),
                handshake.started,
                || {
                    let _ = child.start_kill();
                },
            ) {
                Ok(reader) => reader,
                Err(error) => {
                    let _ = child.start_kill();
                    return Err(error);
                }
            };
        }
        if plan.stdin_policy() == StdinPolicy::Closed {
            input.take();
        }
        Some(reader)
    } else {
        stdout
    };
    let stdout = stdout.map(|reader| OutputReader {
        stream: if matches!(plan.io_mode(), crate::IoMode::Pty(_)) {
            OutputStream::Terminal
        } else {
            OutputStream::Stdout
        },
        reader,
    });
    let stderr = child
        .stderr()
        .take()
        .map(|reader| OutputReader { stream: OutputStream::Stderr, reader: Box::new(reader) });
    let mut readers = Vec::with_capacity(2);
    if let Some(stdout) = stdout {
        readers.push(stdout);
    }
    if let Some(stderr) = stderr {
        readers.push(stderr);
    }
    let identity =
        ProcessTreeIdentity::new(root_pid, current_start_token(root_pid), Some(root_pid), true);
    Ok(Box::new(PipeProcess {
        #[cfg(unix)]
        child,
        #[cfg(windows)]
        child: Some(child),
        identity,
        input,
        readers,
        #[cfg(windows)]
        termination_requested: false,
        #[cfg(windows)]
        job_reap: None,
        #[cfg(windows)]
        job_reaped: false,
        #[cfg(windows)]
        windows_channels,
        #[cfg(windows)]
        windows_terminal: matches!(plan.io_mode(), crate::IoMode::Pty(_)),
    }))
}

struct PipeProcess {
    #[cfg(unix)]
    child: Box<dyn ChildWrapper>,
    #[cfg(windows)]
    child: Option<Box<dyn ChildWrapper>>,
    identity: ProcessTreeIdentity,
    input: Option<Box<dyn Write + Send>>,
    readers: Vec<OutputReader>,
    #[cfg(windows)]
    termination_requested: bool,
    #[cfg(windows)]
    job_reap: Option<WindowsJobReap>,
    #[cfg(windows)]
    job_reaped: bool,
    #[cfg(windows)]
    windows_channels: Option<crate::NativeWindowsHelperChannels>,
    #[cfg(windows)]
    windows_terminal: bool,
}

#[cfg(windows)]
struct WindowsJobReap {
    completion: Receiver<std::io::Result<std::process::ExitStatus>>,
    task: JoinHandle<()>,
}

impl PlatformProcess for PipeProcess {
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
        #[cfg(unix)]
        let status = self.child.try_wait().map_err(|_| tree_error("pipe process wait failed"))?;
        #[cfg(windows)]
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| tree_error("pipe process is already being reaped"))?
            .try_wait()
            .map_err(|_| tree_error("pipe process wait failed"))?;
        Ok(status.map(convert_status))
    }

    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError> {
        self.input.take();
        #[cfg(unix)]
        {
            let signal = match action {
                GracefulAction::CloseInput => return Ok(()),
                GracefulAction::Interrupt => 2,
                GracefulAction::Terminate => 15,
            };
            self.child.signal(signal).map_err(|_| tree_error("process-group signal failed"))
        }
        #[cfg(not(unix))]
        {
            if self.windows_terminal
                && let Some(channels) = &self.windows_channels
            {
                return channels.graceful(action);
            }
            match action {
                GracefulAction::CloseInput => Ok(()),
                GracefulAction::Interrupt | GracefulAction::Terminate => {
                    self.request_job_termination("job termination request failed")
                }
            }
        }
    }

    fn force_kill(&mut self) -> Result<(), ProcessError> {
        self.input.take();
        #[cfg(unix)]
        {
            self.child.start_kill().map_err(|_| tree_error("forced process-tree kill failed"))
        }
        #[cfg(windows)]
        {
            self.request_job_termination("forced process-tree kill failed")
        }
    }

    fn tree_quiescent(&mut self) -> Result<bool, ProcessError> {
        #[cfg(unix)]
        {
            super::process_group_quiescent(self.identity)
        }
        #[cfg(windows)]
        {
            self.poll_job_reap()
        }
    }

    fn process_count(&mut self) -> Result<Option<u64>, ProcessError> {
        #[cfg(unix)]
        {
            super::process_group_count(self.identity)
        }
        #[cfg(windows)]
        {
            Ok(None)
        }
    }

    fn resize(&mut self, size: TerminalSize) -> Result<(), ProcessError> {
        #[cfg(windows)]
        if self.windows_terminal
            && let Some(channels) = &self.windows_channels
        {
            return channels.resize(size);
        }
        #[cfg(not(windows))]
        let _ = size;
        Err(ProcessError::new(
            ErrorCode::InvalidInput,
            ProcessOperation::Control,
            RecoveryClass::CorrectRequest,
            "pipe process cannot be resized",
        ))
    }
}

#[cfg(windows)]
impl PipeProcess {
    fn request_job_termination(&mut self, detail: &'static str) -> Result<(), ProcessError> {
        if self.termination_requested || self.job_reaped {
            return Ok(());
        }
        self.child
            .as_mut()
            .ok_or_else(|| tree_error("pipe process job handle is unavailable"))?
            .start_kill()
            .map_err(|_| tree_error(detail))?;
        self.termination_requested = true;
        Ok(())
    }

    fn poll_job_reap(&mut self) -> Result<bool, ProcessError> {
        if self.job_reaped {
            return Ok(true);
        }
        if !self.termination_requested {
            return Ok(false);
        }
        if self.job_reap.is_none() {
            self.start_job_reap()?;
        }
        let Some(reap) = self.job_reap.as_ref() else {
            return Ok(false);
        };
        let result = match reap.completion.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                let reap = self
                    .job_reap
                    .take()
                    .ok_or_else(|| tree_error("job reap task is unavailable"))?;
                reap.task.join().map_err(|_| tree_error("Windows job reap task panicked"))?;
                return Err(tree_error("Windows job reap task disconnected"));
            }
        };
        let reap =
            self.job_reap.take().ok_or_else(|| tree_error("job reap task is unavailable"))?;
        reap.task.join().map_err(|_| tree_error("Windows job reap task panicked"))?;
        result.map_err(|_| tree_error("Windows job completion wait failed"))?;
        self.job_reaped = true;
        Ok(true)
    }

    fn start_job_reap(&mut self) -> Result<(), ProcessError> {
        let mut child = self
            .child
            .take()
            .ok_or_else(|| tree_error("pipe process job handle is unavailable"))?;
        let (completion, observation) = sync_channel(1);
        let task = thread::Builder::new()
            .name("peritus-windows-job-reap".to_owned())
            .spawn(move || {
                let _ = completion.send(child.wait());
            })
            .map_err(|_| tree_error("Windows job reap task cannot be started"))?;
        self.job_reap = Some(WindowsJobReap { completion: observation, task });
        Ok(())
    }
}

fn convert_status(status: std::process::ExitStatus) -> PlatformExit {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return PlatformExit::Signal(signal);
        }
    }
    status.code().map_or(PlatformExit::PlatformException(1), PlatformExit::Code)
}

const fn spawn_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Spawn,
        ProcessOperation::Spawn,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

const fn tree_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ProcessTree,
        ProcessOperation::Control,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
