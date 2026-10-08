//! Platform resource observations used by supervisor enforcement.

#[cfg(any(target_os = "macos", all(test, unix)))]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::process_group_count;

use crate::{
    ErrorCode, ProcessError, ProcessOperation, RecoveryClass, platform::ProcessTreeIdentity,
};

/// One local supervisor resource sample, with unavailable dimensions omitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlatformResourceSample {
    cpu_millis: Option<u64>,
    memory_bytes: Option<u64>,
    process_count: Option<u64>,
    open_handles: Option<u64>,
}

impl PlatformResourceSample {
    pub(crate) const fn cpu_millis(self) -> Option<u64> {
        self.cpu_millis
    }
    pub(crate) const fn memory_bytes(self) -> Option<u64> {
        self.memory_bytes
    }
    pub(crate) const fn process_count(self) -> Option<u64> {
        self.process_count
    }
    pub(crate) const fn open_handles(self) -> Option<u64> {
        self.open_handles
    }
}

/// Resource dimensions the selected execution path can enforce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlatformResourceCapabilities {
    wall_time: bool,
    cpu_time: bool,
    memory: bool,
    disk: bool,
    output: bool,
    process_count: bool,
    open_handles: bool,
    concurrency: bool,
}

impl PlatformResourceCapabilities {
    #[cfg(target_os = "linux")]
    const fn all() -> Self {
        Self {
            wall_time: true,
            cpu_time: true,
            memory: true,
            disk: true,
            output: true,
            process_count: true,
            open_handles: true,
            concurrency: true,
        }
    }

    pub(crate) fn from_backend_features(features: peritus_sandbox::FeatureSet) -> Self {
        use peritus_sandbox::SandboxFeature;

        Self {
            wall_time: features.contains(SandboxFeature::WallTime),
            cpu_time: features.contains(SandboxFeature::CpuTime),
            memory: features.contains(SandboxFeature::Memory),
            disk: features.contains(SandboxFeature::Disk),
            output: features.contains(SandboxFeature::Output),
            process_count: features.contains(SandboxFeature::ProcessCount),
            open_handles: features.contains(SandboxFeature::OpenHandles),
            concurrency: features.contains(SandboxFeature::Concurrency),
        }
    }

    pub(crate) const fn wall_time(self) -> bool {
        self.wall_time
    }

    pub(crate) const fn cpu_time(self) -> bool {
        self.cpu_time
    }

    pub(crate) const fn memory(self) -> bool {
        self.memory
    }

    pub(crate) const fn disk(self) -> bool {
        self.disk
    }

    pub(crate) const fn output(self) -> bool {
        self.output
    }

    pub(crate) const fn process_count(self) -> bool {
        self.process_count
    }

    pub(crate) const fn open_handles(self) -> bool {
        self.open_handles
    }

    pub(crate) const fn concurrency(self) -> bool {
        self.concurrency
    }

    pub(crate) const fn has_sampler(self) -> bool {
        self.cpu_time || self.memory || self.disk || self.process_count || self.open_handles
    }
}

#[cfg(target_os = "linux")]
pub(crate) const fn local_resource_capabilities() -> PlatformResourceCapabilities {
    PlatformResourceCapabilities::all()
}

#[cfg(target_os = "macos")]
pub(crate) const fn local_resource_capabilities() -> PlatformResourceCapabilities {
    PlatformResourceCapabilities {
        wall_time: true,
        cpu_time: false,
        memory: false,
        disk: true,
        output: true,
        process_count: true,
        open_handles: false,
        concurrency: true,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) const fn local_resource_capabilities() -> PlatformResourceCapabilities {
    PlatformResourceCapabilities {
        wall_time: true,
        cpu_time: false,
        memory: false,
        disk: false,
        output: true,
        process_count: false,
        open_handles: false,
        concurrency: true,
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn sample_resources(
    identity: ProcessTreeIdentity,
) -> Result<PlatformResourceSample, ProcessError> {
    let group = identity
        .process_group()
        .ok_or_else(|| sample_error("process-group identity is unavailable"))?;
    let processes = group_members(group)?;
    let cpu_nanos = sum_available(&processes, cpu_nanos_for);
    let memory_bytes = sum_available(&processes, memory_bytes_for);
    let open_handles = sum_available(&processes, open_handles_for);
    Ok(PlatformResourceSample {
        cpu_millis: cpu_nanos.map(|value| value / 1_000_000),
        memory_bytes,
        process_count: Some(u64::try_from(processes.len()).unwrap_or(u64::MAX)),
        open_handles,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn sample_resources(
    identity: ProcessTreeIdentity,
) -> Result<PlatformResourceSample, ProcessError> {
    Ok(PlatformResourceSample {
        cpu_millis: None,
        memory_bytes: None,
        process_count: macos::process_group_count(identity)?,
        open_handles: None,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) const fn sample_resources(
    _identity: ProcessTreeIdentity,
) -> Result<PlatformResourceSample, ProcessError> {
    Err(sample_error("local supervisor resource sampling is unavailable on this platform"))
}

#[cfg(target_os = "linux")]
pub(crate) fn process_group_count(
    identity: ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    sample_resources(identity).map(PlatformResourceSample::process_count)
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
    for entry in entries {
        let entry = entry.map_err(|_| sample_error("process table cannot be observed"))?;
        let Some(process) = entry.file_name().to_str().and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let stat = match std::fs::read_to_string(format!("/proc/{process}/stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(sample_error("process table cannot be observed")),
        };
        let close = stat
            .rfind(')')
            .ok_or_else(|| sample_error("process table contains malformed entries"))?;
        let process_group = stat
            .get(close + 2..)
            .and_then(|tail| tail.split_ascii_whitespace().nth(2))
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| sample_error("process table contains malformed entries"))?;
        if process_group == group {
            processes.push(process);
        }
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn cpu_nanos_for(process: u32) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{process}/schedstat"))
        .ok()
        .and_then(|value| value.split_ascii_whitespace().next()?.parse().ok())
}

#[cfg(target_os = "linux")]
fn memory_bytes_for(process: u32) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{process}/status"))
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let value = line.strip_prefix("VmRSS:")?.split_ascii_whitespace().next()?;
                value.parse::<u64>().ok()?.checked_mul(1_024)
            })
        })
}

#[cfg(target_os = "linux")]
fn open_handles_for(process: u32) -> Option<u64> {
    std::fs::read_dir(format!("/proc/{process}/fd"))
        .ok()
        .and_then(|mut entries| {
            entries.try_fold(0_u64, |count, entry| {
                entry.ok()?;
                Some(count.saturating_add(1))
            })
        })
}

#[cfg(target_os = "linux")]
fn sum_available(
    processes: &[u32],
    observe: impl Fn(u32) -> Option<u64>,
) -> Option<u64> {
    processes.iter().try_fold(0_u64, |total, process| {
        Some(total.saturating_add(observe(*process)?))
    })
}

const fn sample_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ResourceLimit,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
