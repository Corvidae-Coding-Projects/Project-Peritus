//! Own the complete compiler/engine process group and retain incomplete status on interruption.

use super::write_json;
use crate::error::XtaskError;
use process_wrap::std::{ChildWrapper, CommandWrap};
use serde_json::json;
use std::fs::File;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct Outcome {
    pub(super) status: ExitStatus,
    pub(super) timed_out: bool,
}

struct OwnedChild(Option<Box<dyn ChildWrapper>>);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.start_kill();
            let _ = child.wait();
        }
    }
}

pub(super) fn checked(
    root: &Path,
    evidence: &Path,
    label: &str,
    command: Command,
    seconds: u64,
) -> Result<(), XtaskError> {
    let outcome = run(root, evidence, label, command, Duration::from_secs(seconds))?;
    if outcome.timed_out || !outcome.status.success() {
        return Err(XtaskError::metadata(format!(
            "discovery {label}: exit {}, timeout {}; inspect {}",
            outcome.status,
            outcome.timed_out,
            evidence.display()
        )));
    }
    Ok(())
}

pub(super) fn run(
    root: &Path,
    evidence: &Path,
    label: &str,
    mut command: Command,
    budget: Duration,
) -> Result<Outcome, XtaskError> {
    let report = evidence.join(format!("{label}.json"));
    let stdout_path = evidence.join(format!("{label}.stdout"));
    let stderr_path = evidence.join(format!("{label}.stderr"));
    let stdout = File::create(&stdout_path)
        .map_err(|error| XtaskError::io("create log", &stdout_path, error))?;
    let stderr = File::create(&stderr_path)
        .map_err(|error| XtaskError::io("create log", &stderr_path, error))?;
    let scratch = evidence.join("scratch");
    std::fs::create_dir_all(&scratch)
        .map_err(|error| XtaskError::io("create owned scratch", &scratch, error))?;
    command
        .env("TMPDIR", &scratch)
        .env("TMP", &scratch)
        .env("TEMP", &scratch)
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    let invocation: Vec<_> = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    write_json(
        &report,
        &json!({"status": "incomplete", "argv": invocation, "budget_seconds": budget.as_secs_f64()}),
    )?;
    let started = Instant::now();
    let mut wrap = CommandWrap::from(command);
    #[cfg(unix)]
    wrap.wrap(process_wrap::std::ProcessGroup::leader());
    #[cfg(windows)]
    wrap.wrap(process_wrap::std::JobObject);
    let mut owned = OwnedChild(Some(
        wrap.spawn().map_err(|error| XtaskError::io("spawn discovery command", root, error))?,
    ));
    let child =
        owned.0.as_mut().ok_or_else(|| XtaskError::metadata("discovery child ownership lost"))?;
    let pid = child.id();
    let (status, timed_out) = loop {
        // Observe the direct child's status without consuming group bookkeeping.
        if let Some(status) = child
            .inner_mut()
            .try_wait()
            .map_err(|error| XtaskError::io("poll discovery child", root, error))?
        {
            break (status, false);
        }
        if started.elapsed() >= budget {
            child.start_kill().map_err(|error| {
                XtaskError::io("terminate discovery process group", root, error)
            })?;
            let status = child
                .wait()
                .map_err(|error| XtaskError::io("reap discovery process group", root, error))?;
            break (status, true);
        }
        thread::sleep(Duration::from_millis(20));
    };
    // Also contain workers left behind by an engine that returned early.
    if let Err(error) = child.start_kill() {
        #[cfg(unix)]
        let gone = error.raw_os_error() == Some(3);
        #[cfg(not(unix))]
        let gone = error.kind() == std::io::ErrorKind::NotFound;
        if !gone {
            return Err(XtaskError::io("clean discovery process group", root, error));
        }
    }
    child.wait().map_err(|error| XtaskError::io("reap discovery child", root, error))?;
    owned.0 = None;
    write_json(
        &report,
        &json!({
            "status": if timed_out { "timeout" } else if status.success() { "completed" } else { "command_failed" },
            "argv": invocation, "budget_seconds": budget.as_secs_f64(), "elapsed_seconds": started.elapsed().as_secs_f64(),
            "exit_code": status.code(), "root_pid": pid, "owned_group_cleanup": "kill_requested_and_root_reaped",
        }),
    )?;
    Ok(Outcome { status, timed_out })
}
