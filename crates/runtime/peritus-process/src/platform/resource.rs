//! Platform resource observations used by supervisor enforcement.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::process_group_count;

#[cfg(not(target_os = "linux"))]
use crate::{ErrorCode, ProcessOperation, RecoveryClass};
use crate::{ProcessError, platform::ProcessTreeIdentity};

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
    let processes = super::linux_tree::members(identity)?;
    let mut cpu_nanos = 0_u64;
    let mut memory_bytes = 0_u64;
    let mut open_handles = 0_u64;
    for process in &processes {
        cpu_nanos = cpu_nanos.saturating_add(cpu_nanos_for(process.pid()));
        memory_bytes = memory_bytes.saturating_add(memory_bytes_for(process.pid()));
        open_handles = open_handles.saturating_add(open_handles_for(process.pid()));
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

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub(crate) const fn process_group_count(
    _identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    Ok(None)
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

#[cfg(not(target_os = "linux"))]
const fn sample_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ResourceLimit,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
