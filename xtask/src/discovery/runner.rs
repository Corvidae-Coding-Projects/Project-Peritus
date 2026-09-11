//! Own the complete compiler/engine process group and retain incomplete status on interruption.

use super::write_json;
use crate::error::XtaskError;
use process_wrap::std::{ChildWrapper, CommandWrap};
use serde_json::json;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct Outcome {
    pub(super) status: ExitStatus,
    pub(super) timed_out: bool,
}

struct OwnedChild(Option<Box<dyn ChildWrapper>>);

struct OwnedScratch(Option<PathBuf>);

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.start_kill();
            let _ = child.wait();
        }
    }
}

impl OwnedScratch {
    fn create(root: &Path) -> Result<Self, XtaskError> {
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
        let path = canonical_temporary.join(format!(
            "pd-{}-{}",
            process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
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
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
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
    let compiler_state = evidence.join("scratch");
    let object_cache = compiler_state.join("ccache");
    let object_temp = compiler_state.join("ccache-tmp");
    fs::create_dir_all(&object_cache)
        .and_then(|()| fs::create_dir_all(&object_temp))
        .map_err(|error| XtaskError::io("create owned compiler cache", &compiler_state, error))?;
    let mut scratch = OwnedScratch::create(root)?;
    command
        .env("TMPDIR", scratch.path()?)
        .env("TMP", scratch.path()?)
        .env("TEMP", scratch.path()?)
        .env("CCACHE_DIR", &object_cache)
        .env("CCACHE_TEMPDIR", &object_temp)
        .env("GIT_CEILING_DIRECTORIES", scratch.path()?)
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
    scratch.remove()?;
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
