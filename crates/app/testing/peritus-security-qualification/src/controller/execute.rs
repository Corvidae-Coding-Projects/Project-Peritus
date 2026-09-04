//! Sequential execution and bounded capture for one closed probe plan.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use super::error::ControllerError;
use super::inventory::{self, SourceObservation};
use super::plan::{Check, CommandCheck, NativeCheck, ProbePlan};

const MAX_RETAINED_STREAM_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(super) enum CheckRecord {
    Command {
        label: &'static str,
        arguments: Vec<String>,
        passed: bool,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    Native {
        facility: &'static str,
        passed: bool,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    Source {
        passed: bool,
        observation: SourceObservation,
    },
}

pub(super) struct ProbeExecution {
    pub(super) passed: bool,
    pub(super) records: Vec<CheckRecord>,
    pub(super) process_count: u32,
    pub(super) peak_memory_bytes: u64,
    pub(super) output_bytes: u64,
}

pub(super) fn run(
    plan: &ProbePlan,
    candidate_root: &Path,
    output_limit: u64,
) -> Result<ProbeExecution, ControllerError> {
    let capture_limit =
        usize::try_from(output_limit).unwrap_or(usize::MAX).min(MAX_RETAINED_STREAM_BYTES);
    let mut records = Vec::with_capacity(plan.checks.len());
    let mut passed = true;
    let mut output_bytes = 0_u64;
    let mut spawned = false;
    for check in &plan.checks {
        match check {
            Check::Command(command) => {
                let (record, bytes, success) = run_cargo(command, candidate_root, capture_limit);
                records.push(record);
                output_bytes = output_bytes.saturating_add(bytes);
                passed &= success;
                spawned = true;
            }
            Check::Native(facility) => {
                let (record, bytes, success) = run_native(*facility, capture_limit);
                records.push(record);
                output_bytes = output_bytes.saturating_add(bytes);
                passed &= success;
                spawned = true;
            }
            Check::Source(check) => {
                let observation = inventory::run(*check, candidate_root)?;
                records.push(CheckRecord::Source { passed: true, observation });
            }
        }
    }
    if output_bytes > output_limit {
        passed = false;
    }
    Ok(ProbeExecution {
        passed,
        records,
        process_count: if spawned { 2 } else { 1 },
        peak_memory_bytes: observed_peak_memory_bytes(),
        output_bytes,
    })
}

fn run_cargo(check: &CommandCheck, root: &Path, capture_limit: usize) -> (CheckRecord, u64, bool) {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(&check.arguments)
        .env("CARGO_BUILD_JOBS", check.build_jobs.to_string())
        .env("CARGO_NET_OFFLINE", "true")
        .env("RUST_BACKTRACE", "0")
        .output();
    let (status, stdout, stderr, bytes, passed) = summarize(output, capture_limit);
    (
        CheckRecord::Command {
            label: check.label,
            arguments: check.arguments.clone(),
            passed,
            status,
            stdout,
            stderr,
        },
        bytes,
        passed,
    )
}

fn run_native(check: NativeCheck, capture_limit: usize) -> (CheckRecord, u64, bool) {
    let (facility, output) = match check {
        NativeCheck::LinuxBubblewrap => (
            "linux-bubblewrap",
            Command::new("/usr/bin/bwrap")
                .args([
                    "--unshare-all",
                    "--die-with-parent",
                    "--new-session",
                    "--ro-bind",
                    "/",
                    "/",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--",
                    "/bin/true",
                ])
                .output(),
        ),
        NativeCheck::MacosSeatbelt => (
            "macos-seatbelt",
            Command::new("/usr/bin/sandbox-exec")
                .args(["-p", "(version 1) (allow default)", "/usr/bin/true"])
                .output(),
        ),
    };
    let (status, stdout, stderr, bytes, passed) = summarize(output, capture_limit);
    (CheckRecord::Native { facility, passed, status, stdout, stderr }, bytes, passed)
}

fn summarize(
    output: Result<std::process::Output, std::io::Error>,
    capture_limit: usize,
) -> (Option<i32>, String, String, u64, bool) {
    match output {
        Ok(output) => {
            let bytes = output.stdout.len().saturating_add(output.stderr.len());
            let within = bytes <= capture_limit;
            (
                output.status.code(),
                bounded_text(&output.stdout, capture_limit),
                bounded_text(&output.stderr, capture_limit),
                u64::try_from(bytes).unwrap_or(u64::MAX),
                output.status.success() && within,
            )
        }
        Err(error) => (None, String::new(), error.to_string(), 0, false),
    }
}

fn bounded_text(bytes: &[u8], maximum: usize) -> String {
    let count = bytes.len().min(maximum);
    String::from_utf8_lossy(&bytes[..count]).into_owned()
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "read-only getrusage is the controller peak-memory observation boundary"
)]
fn observed_peak_memory_bytes() -> u64 {
    fn usage(which: i32) -> Option<u64> {
        let mut record = std::mem::MaybeUninit::<nix::libc::rusage>::zeroed();
        // SAFETY: `record` is writable for one rusage and is read only after a successful call.
        if unsafe { nix::libc::getrusage(which, record.as_mut_ptr()) } != 0 {
            return None;
        }
        // SAFETY: getrusage initialized the complete record on success.
        let value = unsafe { record.assume_init() }.ru_maxrss;
        u64::try_from(value).ok()
    }
    let self_peak = usage(nix::libc::RUSAGE_SELF).unwrap_or(0);
    let child_peak = usage(nix::libc::RUSAGE_CHILDREN).unwrap_or(0);
    let observed = self_peak.saturating_add(child_peak);
    if cfg!(target_os = "macos") { observed } else { observed.saturating_mul(1_024) }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "read-only process counters are the Windows controller memory observation boundary"
)]
fn observed_peak_memory_bytes() -> u64 {
    use std::mem::size_of;
    use windows_sys::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap_or(u32::MAX),
        ..PROCESS_MEMORY_COUNTERS::default()
    };
    // SAFETY: the current-process pseudo handle is always valid and `counters` is correctly sized.
    let read = unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), counters.cb)
    };
    if read == 0 { 0 } else { u64::try_from(counters.PeakWorkingSetSize).unwrap_or(u64::MAX) }
}

#[cfg(not(any(unix, target_os = "windows")))]
const fn observed_peak_memory_bytes() -> u64 {
    0
}
