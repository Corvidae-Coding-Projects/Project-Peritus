//! Direct structured pipe process launch with process-tree containment.

use std::{
    io::Write,
    process::{Command, Stdio},
};
#[cfg(unix)]
use std::os::fd::AsRawFd;

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessSession;
use process_wrap::std::{ChildWrapper, CommandWrap};

use crate::{
    CommandSpec, ErrorCode, GracefulAction, OutputStream, ProcessError,
    ProcessOperation, RecoveryClass, StdinPolicy, TerminalSize,
    supervisor::SupervisorPlan,
};

use super::{
    HandshakeError, NativeHandshake, NativeHandshakeStatus, OutputReader, PlatformExit,
    PlatformLaunch, PlatformProcess, ProcessInput, ProcessTreeIdentity, current_start_token,
};

#[cfg(windows)]
mod windows_reap;

#[allow(
    clippy::too_many_lines,
    reason = "the direct-child spawn and bounded native handshake are one rollback transaction"
)]
pub(super) fn launch(
    plan: &SupervisorPlan,
    launch_command: &CommandSpec,
    handshake: Option<NativeHandshake>,
    spawned: &mut dyn FnMut(ProcessTreeIdentity) -> Result<(), ProcessError>,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<PlatformLaunch, ProcessError> {
    #[cfg(windows)]
    let native_windows_pty = matches!(plan.io_mode(), crate::IoMode::Pty(_))
        && handshake
            .as_ref()
            .and_then(|value| value.windows_channels.as_ref())
            .is_some();
    #[cfg(not(windows))]
    let native_windows_pty = false;
    let mut command = Command::new(launch_command.executable());
    command
        .args(launch_command.arguments())
        .current_dir(plan.working_directory())
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(if native_windows_pty { Stdio::null() } else { Stdio::piped() });
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    #[cfg(target_os = "linux")]
    super::configure_parent_death(&mut command);
    #[cfg(windows)]
    if let Some(channels) = handshake
        .as_ref()
        .and_then(|value| value.windows_channels.as_ref())
    {
        command.env(crate::NATIVE_WINDOWS_STATUS_HANDLE_ENV, channels.status_handle().to_string());
        command
            .env(crate::NATIVE_WINDOWS_CONTROL_HANDLE_ENV, channels.control_handle().to_string());
        if let Some(job_handle) = channels.containment_job_handle() {
            command.env(crate::NATIVE_WINDOWS_JOB_HANDLE_ENV, job_handle.to_string());
        }
    }
    let protected_handles =
        handshake.as_ref().map_or(&[][..], |value| value.protected_handles.as_slice());
    match (handshake.is_some(), plan.stdin_policy()) {
        (false, StdinPolicy::Closed) => {
            command.stdin(Stdio::null());
        }
        (false, StdinPolicy::Bounded { .. } | StdinPolicy::Streaming { .. }) | (true, _) => {
            command.stdin(Stdio::piped());
        }
    }
    #[cfg(windows)]
    let windows_channels = handshake
        .as_ref()
        .and_then(|value| value.windows_channels.clone());
    #[cfg(windows)]
    let windows_completion = handshake
        .as_ref()
        .map(|value| (value.completion, value.quiesced, value.worker_failed));
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
    #[cfg(unix)]
    let process_group = Some(root_pid);
    #[cfg(windows)]
    let process_group = None;
    let identity =
        ProcessTreeIdentity::new(root_pid, current_start_token(root_pid), process_group, true);
    let mut input = child.stdin().take();
    let child_stdout = child.stdout().take();
    #[cfg(unix)]
    let protocol_descriptors_ready = handshake.is_none()
        || (child_stdout
            .as_ref()
            .is_some_and(|reader| super::set_protocol_nonblocking(reader.as_raw_fd()).is_ok())
            && input
                .as_ref()
                .is_some_and(|writer| super::set_protocol_nonblocking(writer.as_raw_fd()).is_ok()));
    #[cfg(not(unix))]
    let protocol_descriptors_ready = true;
    let mut stdout =
        child_stdout.map(|reader| Box::new(reader) as Box<dyn std::io::Read + Send>);
    let mut handshake_status = NativeHandshakeStatus::Complete;
    if let Some(handshake) = handshake {
        if spawned(identity).is_err() {
            handshake_status = NativeHandshakeStatus::Failed;
            stdout.take();
        }
        if matches!(handshake_status, NativeHandshakeStatus::Complete)
            && !protocol_descriptors_ready
        {
            handshake_status = NativeHandshakeStatus::Failed;
            stdout.take();
        }
        if matches!(handshake_status, NativeHandshakeStatus::Complete) {
            let exchange = (|| {
                let reader = stdout.take().ok_or_else(|| {
                    HandshakeError::Failed(spawn_error(
                        "native helper has no activation output stream",
                    ))
                })?;
                let reader =
                    super::verify_helper_record(reader, handshake.ready, should_continue)?;
                let writer = input
                    .as_mut()
                    .map(|writer| writer as &mut dyn Write)
                    .ok_or_else(|| {
                        HandshakeError::Failed(spawn_error(
                            "native helper has no manifest input stream",
                        ))
                    })?;
                super::write_helper_manifest(writer, &handshake.manifest, should_continue)?;
                super::verify_helper_record(reader, handshake.activated, should_continue)
            })();
            match exchange {
                Ok(reader) => stdout = Some(reader),
                Err(error) => handshake_status = error.status(),
            }
        }
        #[cfg(windows)]
        if matches!(handshake_status, NativeHandshakeStatus::Complete)
            && let Some(status_reader) = status_reader
        {
            let verification = windows_channels
                .as_ref()
                .ok_or_else(|| {
                    HandshakeError::Failed(spawn_error(
                        "Windows helper channels disappeared before custody transfer",
                    ))
                })
                .and_then(|channels| {
                    let reader = channels.verify_secret_files(
                        Box::new(status_reader),
                        handshake.activated,
                        should_continue,
                    )?;
                    if !should_continue() {
                        return Err(HandshakeError::Cancelled);
                    }
                    channels
                        .acknowledge_secret_files()
                        .map_err(HandshakeError::Failed)?;
                    if let Some(adoption) = handshake.adoption {
                        channels
                        .verify_target_adoption(
                            reader,
                            adoption,
                            identity,
                            should_continue,
                        )
                        .and_then(|reader| {
                            if !should_continue() {
                                return Err(HandshakeError::Cancelled);
                            }
                            channels
                                .acknowledge_target_adoption()
                                .map_err(HandshakeError::Failed)?;
                            super::verify_helper_record(reader, handshake.started, should_continue)
                        })
                    } else {
                        super::verify_helper_record(reader, handshake.started, should_continue)
                    }
                });
            if let Err(error) = verification {
                handshake_status = error.status();
            }
        }
        if plan.stdin_policy() == StdinPolicy::Closed {
            input.take();
        }
    }
    #[cfg(unix)]
    if let Some(writer) = input.as_ref()
        && super::set_input_nonblocking(writer.as_raw_fd()).is_err()
    {
        handshake_status = NativeHandshakeStatus::Failed;
    }
    let input = input.map(|writer| ProcessInput::pipe(Box::new(writer)));
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
    let process = Box::new(PipeProcess {
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
        windows_completion: if matches!(handshake_status, NativeHandshakeStatus::Complete) {
            windows_completion
        } else {
            None
        },
        #[cfg(windows)]
        windows_terminal: matches!(plan.io_mode(), crate::IoMode::Pty(_)),
    });
    Ok(PlatformLaunch::new(process, handshake_status))
}

struct PipeProcess {
    #[cfg(unix)]
    child: Box<dyn ChildWrapper>,
    #[cfg(windows)]
    child: Option<Box<dyn ChildWrapper>>,
    identity: ProcessTreeIdentity,
    input: Option<ProcessInput>,
    readers: Vec<OutputReader>,
    #[cfg(windows)]
    termination_requested: bool,
    #[cfg(windows)]
    job_reap: Option<windows_reap::WindowsJobReap>,
    #[cfg(windows)]
    job_reaped: bool,
    #[cfg(windows)]
    windows_channels: Option<crate::NativeWindowsHelperChannels>,
    #[cfg(windows)]
    windows_completion: Option<(
        peritus_types::Sha256Digest,
        peritus_types::Sha256Digest,
        peritus_types::Sha256Digest,
    )>,
    #[cfg(windows)]
    windows_terminal: bool,
}

impl PlatformProcess for PipeProcess {
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
        #[cfg(unix)]
        let status = self.child.try_wait().map_err(|_| tree_error("pipe process wait failed"))?;
        #[cfg(windows)]
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| tree_error("pipe process is already being reaped"))?;
        #[cfg(windows)]
        let status = try_wait_windows_root(&mut **child)?;
        #[cfg(windows)]
        if status.is_some()
            && let Some((completion, quiesced, worker_failed)) = self.windows_completion.take()
        {
            let verified = self
                .windows_channels
                .as_ref()
                .ok_or_else(|| tree_error("Windows helper completion channel is unavailable"))?
                .verify_helper_completion(completion, quiesced, worker_failed);
            if self.termination_requested {
                return Ok(Some(verified.map_or(
                    PlatformExit::Unavailable,
                    convert_windows_completion,
                )));
            }
            return verified.map(convert_windows_completion).map(Some);
        }
        Ok(status.map(convert_status))
    }

    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError> {
        if action == GracefulAction::CloseInput {
            self.input.take();
        }
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
#[allow(
    unsafe_code,
    reason = "the process-wrap job must retain its completion message while C2 polls only the uniquely borrowed raw root child"
)]
fn try_wait_windows_root(
    child: &mut dyn ChildWrapper,
) -> Result<Option<std::process::ExitStatus>, ProcessError> {
    // Poll only the root handle here. `JobObjectChild::try_wait` also consumes one job
    // completion-port message, which can discard the all-processes-exited notification before the
    // bounded job-reap task calls `wait`.
    // SAFETY: `try_wait` only observes and caches the root exit status. It does not move or replace
    // the raw child, and both std Child and the outer JobObjectChild permit repeated waits after
    // exit. The wrapper remains uniquely borrowed and intact for the later job completion wait.
    unsafe { child.inner_child_mut() }
        .try_wait()
        .map_err(|_| tree_error("pipe process wait failed"))
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

#[cfg(windows)]
fn convert_windows_completion(
    completion: crate::NativeWindowsCompletion,
) -> PlatformExit {
    if let Some(failure) = completion.post_activation_failure() {
        return PlatformExit::NativeFailure(crate::NativeFailureObservation::new(
            completion.target_status(),
            failure,
        ));
    }
    if completion.is_legacy() {
        return PlatformExit::Unavailable;
    }
    match completion.target_status() {
        Some(status) => i32::try_from(status)
            .map_or(PlatformExit::PlatformException(status), PlatformExit::Code),
        None => PlatformExit::Unavailable,
    }
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
