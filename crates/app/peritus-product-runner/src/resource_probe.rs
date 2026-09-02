//! Portable run-level memory and workspace-growth observations.

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(any(windows, all(unix, not(target_os = "linux"))))]
use std::process::Command;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_WORKSPACE_ENTRIES: u64 = 2_000_000;

pub(super) struct RunResourceProbe {
    workspace_root: PathBuf,
    baseline_workspace_bytes: u64,
}

#[derive(Clone, Copy)]
pub(super) struct RunResourceObservation {
    pub(super) workspace: u64,
    pub(super) growth: u64,
    pub(super) peak_rss: u64,
}

impl RunResourceProbe {
    pub(super) fn new(workspace_root: &Path) -> Result<Self, ProductRunnerError> {
        let baseline_workspace_bytes = workspace_bytes(workspace_root)?;
        Ok(Self { workspace_root: workspace_root.to_owned(), baseline_workspace_bytes })
    }

    pub(super) fn observe(&self) -> Result<RunResourceObservation, ProductRunnerError> {
        let workspace_bytes = workspace_bytes(&self.workspace_root)?;
        Ok(RunResourceObservation {
            workspace: workspace_bytes,
            growth: workspace_bytes.saturating_sub(self.baseline_workspace_bytes),
            peak_rss: resident_memory_bytes()?,
        })
    }
}

fn workspace_bytes(root: &Path) -> Result<u64, ProductRunnerError> {
    let mut pending = vec![root.to_owned()];
    let mut entries = 0_u64;
    let mut bytes = 0_u64;
    while let Some(directory) = pending.pop() {
        let children =
            fs::read_dir(&directory).map_err(|error| resource_error(error.to_string()))?;
        for child in children {
            let child = child.map_err(|error| resource_error(error.to_string()))?;
            entries = entries
                .checked_add(1)
                .ok_or_else(|| resource_error("workspace entry accounting overflowed"))?;
            if entries > MAX_WORKSPACE_ENTRIES {
                return Err(resource_error("workspace entry observation exceeded its hard bound"));
            }
            if child.file_name() == ".git" {
                continue;
            }
            let metadata = fs::symlink_metadata(child.path())
                .map_err(|error| resource_error(error.to_string()))?;
            if metadata.file_type().is_dir() {
                pending.push(child.path());
            } else if metadata.file_type().is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| resource_error("workspace byte accounting overflowed"))?;
            }
        }
    }
    Ok(bytes)
}

#[cfg(target_os = "linux")]
fn resident_memory_bytes() -> Result<u64, ProductRunnerError> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|error| resource_error(error.to_string()))?;
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| resource_error("Linux did not report VmHWM in /proc/self/status"))?;
    parse_memory_text(value, 1024)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn resident_memory_bytes() -> Result<u64, ProductRunnerError> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map_err(|error| resource_error(error.to_string()))?;
    parse_memory_output(&output.stdout, 1024, output.status.success())
}

#[cfg(windows)]
fn resident_memory_bytes() -> Result<u64, ProductRunnerError> {
    let expression = format!("(Get-Process -Id {}).WorkingSet64", std::process::id());
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &expression])
        .output()
        .map_err(|error| resource_error(error.to_string()))?;
    parse_memory_output(&output.stdout, 1, output.status.success())
}

#[cfg(any(windows, all(unix, not(target_os = "linux"))))]
fn parse_memory_output(
    output: &[u8],
    multiplier: u64,
    success: bool,
) -> Result<u64, ProductRunnerError> {
    if !success {
        return Err(resource_error("the operating system could not report resident memory"));
    }
    let text = std::str::from_utf8(output)
        .map_err(|_| resource_error("resident-memory output was not UTF-8"))?;
    parse_memory_text(text.trim(), multiplier)
}

fn parse_memory_text(text: &str, multiplier: u64) -> Result<u64, ProductRunnerError> {
    let value = text
        .parse::<u64>()
        .map_err(|_| resource_error("resident-memory output was not an integer"))?;
    value
        .checked_mul(multiplier)
        .ok_or_else(|| resource_error("resident-memory accounting overflowed"))
}

fn resource_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Budget, "observe product-run resources", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_growth_ignores_git_storage_and_does_not_follow_links() {
        let temporary = tempfile::tempdir().expect("resource root");
        fs::create_dir(temporary.path().join(".git")).expect("git directory");
        fs::write(temporary.path().join(".git/object"), vec![0_u8; 128]).expect("git object");
        fs::write(temporary.path().join("source.rs"), b"abc").expect("source");
        let probe = RunResourceProbe::new(temporary.path()).expect("resource probe");

        fs::write(temporary.path().join("artifact.bin"), vec![0_u8; 64]).expect("artifact");
        let observation = probe.observe().expect("resource observation");

        assert_eq!(observation.workspace, 67);
        assert_eq!(observation.growth, 64);
        assert!(observation.peak_rss > 0);
    }
}
