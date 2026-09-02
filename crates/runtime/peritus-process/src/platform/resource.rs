//! Platform resource observations used by supervisor enforcement.

use crate::{
    ErrorCode, ProcessError, ProcessOperation, RecoveryClass, platform::ProcessTreeIdentity,
};

/// One complete local supervisor resource sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlatformResourceSample {
    cpu_millis: u64,
    memory_bytes: u64,
    process_count: u64,
    open_handles: u64,
}

impl PlatformResourceSample {
    pub(crate) const fn cpu_millis(self) -> u64 {
        self.cpu_millis
    }
    pub(crate) const fn memory_bytes(self) -> u64 {
        self.memory_bytes
    }
    pub(crate) const fn process_count(self) -> u64 {
        self.process_count
    }
    pub(crate) const fn open_handles(self) -> u64 {
        self.open_handles
    }
}

#[cfg(target_os = "linux")]
pub(crate) const fn local_supervisor_resources_supported() -> bool {
    true
}

#[cfg(not(target_os = "linux"))]
pub(crate) const fn local_supervisor_resources_supported() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(crate) fn sample_resources(
    identity: ProcessTreeIdentity,
) -> Result<PlatformResourceSample, ProcessError> {
    let group = identity
        .process_group()
        .ok_or_else(|| sample_error("process-group identity is unavailable"))?;
    let processes = group_members(group)?;
    let mut cpu_nanos = 0_u64;
    let mut memory_bytes = 0_u64;
    let mut open_handles = 0_u64;
    for process in &processes {
        cpu_nanos = cpu_nanos.saturating_add(cpu_nanos_for(*process));
        memory_bytes = memory_bytes.saturating_add(memory_bytes_for(*process));
        open_handles = open_handles.saturating_add(open_handles_for(*process));
    }
    Ok(PlatformResourceSample {
        cpu_millis: cpu_nanos / 1_000_000,
        memory_bytes,
        process_count: u64::try_from(processes.len()).unwrap_or(u64::MAX),
        open_handles,
    })
}

#[cfg(not(target_os = "linux"))]
pub(crate) const fn sample_resources(
    _identity: ProcessTreeIdentity,
) -> Result<PlatformResourceSample, ProcessError> {
    Err(sample_error("local supervisor resource sampling is unavailable on this platform"))
}

#[cfg(target_os = "linux")]
pub(crate) fn process_group_count(
    identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    sample_resources(identity).map(|sample| Some(sample.process_count()))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "inventoried libproc read-only process-group enumeration boundary")]
pub(crate) fn process_group_count(
    identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    use std::{ffi::c_void, mem::size_of_val};

    const MAX_GROUP_PROCESSES: usize = 16_384;

    let group = identity
        .process_group()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| sample_error("process-group identity is unavailable"))?;
    let mut pids = vec![0_i32; MAX_GROUP_PROCESSES];
    let buffer_bytes = i32::try_from(size_of_val(pids.as_slice()))
        .map_err(|_| sample_error("process-group buffer exceeds platform capacity"))?;
    // SAFETY: `pids` is writable for `buffer_bytes`; the selector requests process IDs for the
    // exact C2-owned process group and transfers no ownership.
    let count =
        unsafe { libc::proc_listpgrppids(group, pids.as_mut_ptr().cast::<c_void>(), buffer_bytes) };
    if count < 0 {
        // macOS can remove a short-lived child's process-group entry before the supervisor's
        // first optional libproc sample. The root handle remains the exit authority, so an
        // unavailable sample is not an execution failure.
        return Ok(None);
    }
    let count = usize::try_from(count)
        .map_err(|_| sample_error("process-group count exceeds platform capacity"))?;
    if count >= pids.len() {
        return Err(sample_error("process-group observation exceeded its bounded buffer"));
    }
    pids.truncate(count);
    pids.retain(|pid| *pid > 0);
    pids.sort_unstable();
    pids.dedup();
    Ok(Some(u64::try_from(pids.len()).unwrap_or(u64::MAX)))
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub(crate) const fn process_group_count(
    _identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    Ok(None)
}

#[cfg(target_os = "linux")]
fn group_members(group: u32) -> Result<Vec<u32>, ProcessError> {
    let entries =
        std::fs::read_dir("/proc").map_err(|_| sample_error("process table cannot be observed"))?;
    let mut processes = Vec::new();
    for entry in entries.flatten() {
        let Some(process) = entry.file_name().to_str().and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{process}/stat")) else {
            continue;
        };
        let Some(close) = stat.rfind(')') else { continue };
        let process_group = stat
            .get(close + 2..)
            .and_then(|tail| tail.split_ascii_whitespace().nth(2))
            .and_then(|value| value.parse::<u32>().ok());
        if process_group == Some(group) {
            processes.push(process);
        }
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn cpu_nanos_for(process: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{process}/schedstat"))
        .ok()
        .and_then(|value| value.split_ascii_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn memory_bytes_for(process: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{process}/status"))
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let value = line.strip_prefix("VmRSS:")?.split_ascii_whitespace().next()?;
                value.parse::<u64>().ok()?.checked_mul(1_024)
            })
        })
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn open_handles_for(process: u32) -> u64 {
    std::fs::read_dir(format!("/proc/{process}/fd"))
        .ok()
        .map_or(0, |entries| u64::try_from(entries.count()).unwrap_or(u64::MAX))
}

const fn sample_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ResourceLimit,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
