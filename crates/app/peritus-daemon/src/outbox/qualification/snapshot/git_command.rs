//! Fixed, bounded Git subprocess used only to seed and independently inspect qualification state.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{filesystem_error, snapshot_error};

const MAX_GIT_OUTPUT_BYTES: usize = 64 * 1024;

pub(super) fn run_git<'a>(
    cwd: &Path,
    arguments: impl IntoIterator<Item = &'a OsStr>,
) -> Result<Output, DaemonError> {
    let output = run_git_observed(cwd, arguments)?;
    if !output.status.success() {
        return Err(DaemonError::new(
            DaemonErrorCode::Storage,
            DaemonRecovery::Reconcile,
            "qualify Git snapshot commit recovery",
            format!(
                "fixed Git command failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    Ok(output)
}

pub(super) fn run_git_observed<'a>(
    cwd: &Path,
    arguments: impl IntoIterator<Item = &'a OsStr>,
) -> Result<Output, DaemonError> {
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .env_clear()
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    copy_environment(&mut command);
    let output = command.output().map_err(filesystem_error)?;
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES || output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(snapshot_error("Git qualification output exceeded its fixed bound"));
    }
    Ok(output)
}

pub(super) fn reference_value(
    source: &Path,
    reference: &str,
) -> Result<Option<String>, DaemonError> {
    let quiet = run_git_observed(
        source,
        [
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new(reference),
        ],
    )?;
    match quiet.status.code() {
        Some(1) if quiet.stdout.is_empty() => return Ok(None),
        Some(0) => {}
        _ => return Err(snapshot_error("Git could not test the exact snapshot reference")),
    }
    let value = run_git(
        source,
        [
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--hash"),
            OsStr::new(reference),
        ],
    )?;
    scalar(&value.stdout).map(Some)
}

pub(super) fn count_snapshot_refs(source: &Path) -> Result<u64, DaemonError> {
    let output = run_git_observed(
        source,
        [
            OsStr::new("for-each-ref"),
            OsStr::new("--format=%(refname)"),
            OsStr::new("refs/peritus/workspaces"),
        ],
    )?;
    if !output.status.success() {
        return Err(snapshot_error("Git could not enumerate retained snapshot references"));
    }
    u64::try_from(
        output.stdout.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).count(),
    )
    .map_err(|_| snapshot_error("snapshot reference count overflow"))
}

fn scalar(bytes: &[u8]) -> Result<String, DaemonError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| snapshot_error("Git returned non-UTF-8 reference identity"))?
        .trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(snapshot_error("Git returned a malformed reference identity"));
    }
    Ok(value.to_owned())
}

fn copy_environment(command: &mut Command) {
    for name in ["PATH", "SYSTEMROOT", "WINDIR", "PATHEXT", "COMSPEC"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Project Peritus")
        .env("GIT_AUTHOR_EMAIL", "peritus@example.invalid")
        .env("GIT_COMMITTER_NAME", "Project Peritus")
        .env("GIT_COMMITTER_EMAIL", "peritus@example.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC");
}

#[cfg(not(windows))]
const fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(windows)]
const fn null_device() -> &'static str {
    "NUL"
}
