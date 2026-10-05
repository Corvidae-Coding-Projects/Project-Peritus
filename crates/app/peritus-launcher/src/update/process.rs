//! Bounded native child-process execution for update helpers.

use std::{
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

#[cfg(not(windows))]
use std::process::Stdio;

use crate::LauncherError;

const POLL_INTERVAL: Duration = Duration::from_millis(25);
#[cfg(not(windows))]
const MAX_CAPTURED_STDOUT_BYTES: usize = 64 * 1024;

pub(super) fn status(
    command: &mut Command,
    operation: &'static str,
    timeout: Option<Duration>,
) -> Result<ExitStatus, LauncherError> {
    configure_group(command);
    let mut child =
        command.spawn().map_err(|error| LauncherError::Update(format!("{operation}: {error}")))?;
    wait(&mut child, operation, timeout)
}

#[cfg(not(windows))]
pub(super) fn stdout(
    command: &mut Command,
    operation: &'static str,
    timeout: Option<Duration>,
) -> Result<(ExitStatus, Vec<u8>), LauncherError> {
    command.stdout(Stdio::piped());
    configure_group(command);
    let mut child =
        command.spawn().map_err(|error| LauncherError::Update(format!("{operation}: {error}")))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| LauncherError::Update(format!("{operation}: stdout pipe is unavailable")))?;
    let reader = thread::spawn(move || drain_bounded(output));
    let result = wait(&mut child, operation, timeout);
    let captured = reader
        .join()
        .map_err(|_| LauncherError::Update(format!("{operation}: stdout reader panicked")))?
        .map_err(|error| LauncherError::Update(format!("{operation}: read stdout: {error}")))?;
    let status = result?;
    if captured.1 {
        return Err(LauncherError::Update(format!(
            "{operation}: stdout exceeded {MAX_CAPTURED_STDOUT_BYTES} bytes"
        )));
    }
    Ok((status, captured.0))
}

fn wait(
    child: &mut Child,
    operation: &'static str,
    timeout: Option<Duration>,
) -> Result<ExitStatus, LauncherError> {
    let deadline = timeout
        .map(|duration| {
            Instant::now()
                .checked_add(duration)
                .ok_or_else(|| LauncherError::Update(format!("{operation}: timeout overflowed")))
        })
        .transpose()?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    terminate(child);
                    let _ = child.wait();
                    return Err(LauncherError::Update(format!(
                        "{operation}: exceeded its caller-supplied deadline"
                    )));
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(error) => {
                terminate(child);
                let _ = child.wait();
                return Err(LauncherError::Update(format!("{operation}: wait failed: {error}")));
            }
        }
    }
}

#[cfg(not(windows))]
fn drain_bounded(mut output: impl std::io::Read) -> std::io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;
    loop {
        let read = output.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_CAPTURED_STDOUT_BYTES.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    Ok((retained, truncated))
}

#[cfg(unix)]
fn configure_group(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

#[cfg(not(unix))]
const fn configure_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate(child: &mut Child) {
    let Ok(pid) = i32::try_from(child.id()) else {
        let _ = child.kill();
        return;
    };
    if nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGKILL)
        .is_err()
    {
        let _ = child.kill();
    }
}

#[cfg(not(unix))]
fn terminate(child: &mut Child) {
    let _ = child.kill();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn status_terminates_a_hung_process_group_at_the_deadline() {
        let started = Instant::now();
        let error = status(
            Command::new("sh").args(["-c", "sleep 30"]),
            "test hung child",
            Some(Duration::from_millis(50)),
        )
        .expect_err("deadline");
        assert!(error.to_string().contains("deadline"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn stdout_captures_a_bounded_successful_result() {
        let (status, output) = stdout(
            Command::new("sh").args(["-c", "printf 'peritus 1.2.3\\n'"]),
            "test output",
            Some(Duration::from_secs(1)),
        )
        .expect("output");
        assert!(status.success());
        assert_eq!(output, b"peritus 1.2.3\n");
    }
}
