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
    ErrorCode, ExecutionPlan, GracefulAction, OutputStream, ProcessError, ProcessOperation,
    RecoveryClass, StdinPolicy, TerminalSize,
};

use super::{
    OutputReader, PlatformExit, PlatformProcess, ProcessTreeIdentity, current_start_token,
};

pub(super) fn launch(plan: &ExecutionPlan) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    let mut command = Command::new(plan.command().executable());
    command
        .args(plan.command().arguments())
        .current_dir(plan.working_directory().path())
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for variable in plan.environment().variables() {
        command.env(variable.name(), variable.value());
    }
    match plan.stdin_policy() {
        StdinPolicy::Closed => {
            command.stdin(Stdio::null());
        }
        StdinPolicy::Bounded { .. } => {
            command.stdin(Stdio::piped());
        }
    }
    let mut wrapped = CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(ProcessSession);
    #[cfg(windows)]
    wrapped.wrap(JobObject);
    let mut child = wrapped.spawn().map_err(|_| spawn_error("pipe process creation failed"))?;
    let root_pid = child.id();
    let input = child.stdin().take().map(|input| Box::new(input) as Box<dyn Write + Send>);
    let stdout = child
        .stdout()
        .take()
        .map(|reader| OutputReader { stream: OutputStream::Stdout, reader: Box::new(reader) });
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

    fn resize(&mut self, _size: TerminalSize) -> Result<(), ProcessError> {
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
