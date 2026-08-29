//! Bounded structured-command execution with process-tree cleanup.

use std::{
    io,
    process::{Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use peritus_agent::DeveloperLoopError;
use process_wrap::std::CommandWrap;
#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessSession;

use super::{effect::drain_bounded, path::tool};

const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub(super) struct BoundedOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) timed_out: bool,
}

pub(super) fn run(
    mut command: Command,
    timeout: Duration,
) -> Result<BoundedOutput, DeveloperLoopError> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut wrapped = CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(ProcessSession);
    #[cfg(windows)]
    wrapped.wrap(JobObject);
    let mut child = wrapped.spawn().map_err(|error| tool(error.to_string()))?;
    let stdout =
        child.stdout().take().ok_or_else(|| tool("command stdout pipe was not created"))?;
    let stderr =
        child.stderr().take().ok_or_else(|| tool("command stderr pipe was not created"))?;
    let stdout_reader = thread::spawn(move || drain_bounded(stdout));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr));

    let started = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(|error| tool(error.to_string()))?
            && stdout_reader.is_finished()
            && stderr_reader.is_finished()
        {
            break (status, false);
        }
        if started.elapsed() >= timeout {
            child.start_kill().map_err(|error| tool(error.to_string()))?;
            let status = child.wait().map_err(|error| tool(error.to_string()))?;
            break (status, true);
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(started.elapsed())));
    };

    Ok(BoundedOutput {
        status,
        stdout: join_reader(stdout_reader, "stdout")?,
        stderr: join_reader(stderr_reader, "stderr")?,
        timed_out,
    })
}

fn join_reader(
    reader: JoinHandle<io::Result<String>>,
    stream: &str,
) -> Result<String, DeveloperLoopError> {
    reader
        .join()
        .map_err(|_| tool(format!("command {stream} reader panicked")))?
        .map_err(|error| tool(format!("read command {stream}: {error}")))
}
