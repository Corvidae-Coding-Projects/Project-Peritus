//! Own the complete compiler/engine process group and retain incomplete status on interruption.

use super::write_json;
use crate::error::XtaskError;
use process_wrap::std::{ChildWrapper, CommandWrap};
use serde_json::json;
use std::env;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

mod capture;
use capture::{CaptureSummary, CaptureTask};

const LOG_LIMIT_BYTES: u64 = 8 * 1024 * 1024;
const CAPTURE_DRAIN_BUDGET: Duration = Duration::from_secs(5);

pub(super) struct Outcome {
    pub(super) status: ExitStatus,
    pub(super) timed_out: bool,
}

struct OwnedRun {
    child: Option<Box<dyn ChildWrapper>>,
    stdout: Option<CaptureTask>,
    stderr: Option<CaptureTask>,
    capture_drain_budget: Duration,
}

struct OwnedScratch(Option<PathBuf>);

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

impl OwnedRun {
    fn child(&mut self) -> Result<&mut Box<dyn ChildWrapper>, XtaskError> {
        self.child.as_mut().ok_or_else(|| XtaskError::metadata("discovery child ownership lost"))
    }
}

impl Drop for OwnedRun {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
            let _ = child.wait();
        }
        let deadline = Instant::now() + self.capture_drain_budget;
        for task in [&mut self.stdout, &mut self.stderr] {
            if task.is_none() {
                continue;
            }
            while task.as_ref().is_some_and(|task| !task.is_finished()) && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(20));
            }
            if task.as_ref().is_some_and(CaptureTask::is_finished) {
                task.take().expect("finished capture remains owned").cancel_or_join();
            } else {
                task.take().expect("unfinished capture remains owned").cancel_or_join();
            }
        }
    }
}

impl OwnedScratch {
    fn create(root: &Path, evidence: &Path, label: &str) -> Result<Self, XtaskError> {
        let temporary_root = env::temp_dir();
        let canonical_root = root
            .canonicalize()
            .map_err(|error| XtaskError::io("canonicalize repository", root, error))?;
        let canonical_temporary = temporary_root.canonicalize().map_err(|error| {
            XtaskError::io("canonicalize temporary directory", &temporary_root, error)
        })?;
        if canonical_temporary.starts_with(canonical_root) {
            return Err(XtaskError::metadata(
                "discovery temporary directory must be outside the Cargo workspace",
            ));
        }
        let mut identity = DefaultHasher::new();
        evidence.hash(&mut identity);
        label.hash(&mut identity);
        process::id().hash(&mut identity);
        NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed).hash(&mut identity);
        let path = canonical_temporary.join(format!("pd-{:016x}", identity.finish()));
        fs::create_dir(&path)
            .map_err(|error| XtaskError::io("create isolated temporary directory", &path, error))?;
        Ok(Self(Some(path)))
    }

    fn path(&self) -> Result<&Path, XtaskError> {
        self.0.as_deref().ok_or_else(|| XtaskError::metadata("discovery temporary ownership lost"))
    }

    fn remove(&mut self) -> Result<(), XtaskError> {
        let path = self
            .0
            .as_ref()
            .ok_or_else(|| XtaskError::metadata("discovery temporary ownership lost"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match fs::remove_dir_all(path) {
                Ok(()) => {
                    self.0 = None;
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    self.0 = None;
                    return Ok(());
                }
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    return Err(XtaskError::io(
                        "remove quiescent isolated temporary directory",
                        path,
                        error,
                    ));
                }
            }
        }
    }
}

impl Drop for OwnedScratch {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
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
    command: Command,
    budget: Duration,
) -> Result<Outcome, XtaskError> {
    run_with_log_limit(root, evidence, label, command, budget, LOG_LIMIT_BYTES)
}

pub(super) fn run_with_log_limit(
    root: &Path,
    evidence: &Path,
    label: &str,
    command: Command,
    budget: Duration,
    log_limit: u64,
) -> Result<Outcome, XtaskError> {
    run_with_limits(root, evidence, label, command, budget, log_limit, CAPTURE_DRAIN_BUDGET)
}

pub(super) fn run_with_limits(
    root: &Path,
    evidence: &Path,
    label: &str,
    mut command: Command,
    budget: Duration,
    log_limit: u64,
    capture_drain_budget: Duration,
) -> Result<Outcome, XtaskError> {
    let report = evidence.join(format!("{label}.json"));
    let stdout_path = evidence.join(format!("{label}.stdout"));
    let stderr_path = evidence.join(format!("{label}.stderr"));
    let stdout = File::create(&stdout_path)
        .map_err(|error| XtaskError::io("create log", &stdout_path, error))?;
    let stderr = File::create(&stderr_path)
        .map_err(|error| XtaskError::io("create log", &stderr_path, error))?;
    let compiler_state = evidence.join("scratch");
    let object_cache = compiler_state.join("ccache");
    let object_temp = compiler_state.join("ccache-tmp");
    fs::create_dir_all(&object_cache)
        .and_then(|()| fs::create_dir_all(&object_temp))
        .map_err(|error| XtaskError::io("create owned compiler cache", &compiler_state, error))?;
    let mut scratch = OwnedScratch::create(root, evidence, label)?;
    prepare_command(root, &scratch, &object_cache, &object_temp, &mut command)?;
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
    let mut owned =
        spawn_owned_run(root, &mut wrap, stdout, stderr, log_limit, capture_drain_budget)?;
    let (pid, status, timed_out) = {
        let child = owned.child()?;
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
            let gone = error.kind() == io::ErrorKind::NotFound;
            if !gone {
                return Err(XtaskError::io("clean discovery process group", root, error));
            }
        }
        child.wait().map_err(|error| XtaskError::io("reap discovery child", root, error))?;
        (pid, status, timed_out)
    };
    owned.child = None;
    let capture_deadline = Instant::now() + capture_drain_budget;
    let stdout_capture =
        capture::finish(&mut owned.stdout, "stdout", &stdout_path, capture_deadline)?;
    let stderr_capture =
        capture::finish(&mut owned.stderr, "stderr", &stderr_path, capture_deadline)?;
    scratch.remove()?;
    write_terminal_report(
        &report,
        &invocation,
        budget,
        started.elapsed(),
        status,
        timed_out,
        pid,
        log_limit,
        &stdout_capture,
        &stderr_capture,
    )?;
    Ok(Outcome { status, timed_out })
}

#[allow(clippy::too_many_arguments, reason = "terminal report records one completed child run")]
fn write_terminal_report(
    report: &Path,
    invocation: &[String],
    budget: Duration,
    elapsed: Duration,
    status: ExitStatus,
    timed_out: bool,
    pid: u32,
    log_limit: u64,
    stdout: &CaptureSummary,
    stderr: &CaptureSummary,
) -> Result<(), XtaskError> {
    write_json(
        report,
        &json!({
            "status": if timed_out { "timeout" } else if status.success() { "completed" } else { "command_failed" },
            "argv": invocation, "budget_seconds": budget.as_secs_f64(),
            "elapsed_seconds": elapsed.as_secs_f64(), "exit_code": status.code(), "root_pid": pid,
            "process_cleanup": {
                "containment": containment_kind(), "kill_requested_after_root_exit": true,
                "root_reaped": true, "capture_pipes_closed": true,
                "detached_descendant_census": "campaign-specific evidence required",
            },
            "stdout": capture::report(stdout, log_limit), "stderr": capture::report(stderr, log_limit),
        }),
    )
}

const fn containment_kind() -> &'static str {
    if cfg!(unix) {
        "unix_process_group"
    } else if cfg!(windows) {
        "windows_job_object"
    } else {
        "root_process_only"
    }
}

fn prepare_command(
    root: &Path,
    scratch: &OwnedScratch,
    object_cache: &Path,
    object_temp: &Path,
    command: &mut Command,
) -> Result<(), XtaskError> {
    command
        .env("TMPDIR", scratch.path()?)
        .env("TMP", scratch.path()?)
        .env("TEMP", scratch.path()?)
        .env("CCACHE_DIR", object_cache)
        .env("CCACHE_TEMPDIR", object_temp)
        .env("GIT_CEILING_DIRECTORIES", scratch.path()?)
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(())
}

fn spawn_owned_run(
    root: &Path,
    wrap: &mut CommandWrap,
    stdout: File,
    stderr: File,
    log_limit: u64,
    capture_drain_budget: Duration,
) -> Result<OwnedRun, XtaskError> {
    let child =
        wrap.spawn().map_err(|error| XtaskError::io("spawn discovery command", root, error))?;
    let mut owned =
        OwnedRun { child: Some(child), stdout: None, stderr: None, capture_drain_budget };
    let stdout_pipe = owned
        .child()?
        .stdout()
        .take()
        .ok_or_else(|| XtaskError::metadata("discovery child stdout pipe is missing"))?;
    owned.stdout = Some(capture::spawn("discovery-stdout", stdout_pipe, stdout, log_limit)?);
    let stderr_pipe = owned
        .child()?
        .stderr()
        .take()
        .ok_or_else(|| XtaskError::metadata("discovery child stderr pipe is missing"))?;
    owned.stderr = Some(capture::spawn("discovery-stderr", stderr_pipe, stderr, log_limit)?);
    Ok(owned)
}
