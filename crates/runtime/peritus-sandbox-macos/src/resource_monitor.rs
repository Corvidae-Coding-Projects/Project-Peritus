//! Bounded macOS process-group and workspace resource sampling.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

#[cfg(target_os = "macos")]
use std::time::Duration;

use peritus_process::ProcessTreeIdentity;
#[cfg(any(target_os = "macos", test))]
use peritus_sandbox::SandboxResourceKind;

use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction, ResourceControlPlan};

#[cfg(target_os = "macos")]
const DISK_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(target_os = "macos")]
const MAX_DISK_ENTRIES: usize = 1_000_000;

#[cfg(target_os = "macos")]
pub(crate) fn native_controls_available() -> bool {
    native::controls_available()
}

pub(crate) struct ResourceMonitor {
    workspace: PathBuf,
    baseline_disk: u64,
    last_disk_sample: Option<Instant>,
    greatest: ResourceUsage,
}

impl ResourceMonitor {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "macOS construction performs a fallible bounded disk baseline"
    )]
    pub(crate) fn new(workspace: &Path) -> Result<Self, MacosError> {
        #[cfg(target_os = "macos")]
        let baseline_disk = disk_usage(workspace)?;
        #[cfg(not(target_os = "macos"))]
        let baseline_disk = 0;
        Ok(Self {
            workspace: workspace.to_path_buf(),
            baseline_disk,
            last_disk_sample: None,
            greatest: ResourceUsage::default(),
        })
    }

    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "macOS polling updates monotonic resource peaks"
    )]
    pub(crate) fn poll(
        &mut self,
        tree: ProcessTreeIdentity,
        controls: &ResourceControlPlan,
    ) -> Result<bool, MacosError> {
        #[cfg(target_os = "macos")]
        {
            let mut current = native::sample_process_group(tree)?;
            if self.last_disk_sample.is_none_or(|sample| sample.elapsed() >= DISK_SAMPLE_INTERVAL) {
                current.disk_bytes =
                    disk_usage(&self.workspace)?.saturating_sub(self.baseline_disk);
                self.last_disk_sample = Some(Instant::now());
            } else {
                current.disk_bytes = self.greatest.disk_bytes;
            }
            self.greatest.merge(current);
            Ok(exceeds(&self.greatest, controls))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (
                tree,
                controls,
                &self.workspace,
                self.baseline_disk,
                self.last_disk_sample,
                self.greatest,
            );
            Err(MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Activate,
                RecoveryAction::SelectSupportedBackend,
                "macOS process-group resource sampling is unavailable",
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ResourceUsage {
    cpu_nanos: u64,
    memory_bytes: u64,
    disk_bytes: u64,
    open_handles: u64,
    processes: u64,
}

impl ResourceUsage {
    #[cfg(target_os = "macos")]
    fn merge(&mut self, current: Self) {
        self.cpu_nanos = self.cpu_nanos.max(current.cpu_nanos);
        self.memory_bytes = self.memory_bytes.max(current.memory_bytes);
        self.disk_bytes = self.disk_bytes.max(current.disk_bytes);
        self.open_handles = self.open_handles.max(current.open_handles);
        self.processes = self.processes.max(current.processes);
    }
}

#[cfg(any(target_os = "macos", test))]
const fn exceeds(usage: &ResourceUsage, controls: &ResourceControlPlan) -> bool {
    let cpu_nanos =
        controls.control(SandboxResourceKind::CpuTime).ceiling().saturating_mul(1_000_000);
    usage.cpu_nanos > cpu_nanos
        || usage.memory_bytes > controls.control(SandboxResourceKind::Memory).ceiling()
        || usage.disk_bytes > controls.control(SandboxResourceKind::Disk).ceiling()
        || usage.open_handles > controls.control(SandboxResourceKind::OpenHandles).ceiling()
        || usage.processes > controls.control(SandboxResourceKind::Processes).ceiling()
}

#[cfg(target_os = "macos")]
fn disk_usage(root: &Path) -> Result<u64, MacosError> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0_u64;
    let mut visited = 0_usize;
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(directory).map_err(|_| sample_error())?;
        for entry in entries {
            let entry = entry.map_err(|_| sample_error())?;
            visited = visited.saturating_add(1);
            if visited > MAX_DISK_ENTRIES {
                return Err(MacosError::new(
                    MacosErrorKind::LimitExceeded,
                    MacosOperation::Activate,
                    RecoveryAction::CancelAndReap,
                    "workspace resource sample exceeds its entry bound",
                ));
            }
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(|_| sample_error())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

#[cfg(target_os = "macos")]
fn sample_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::SupervisorFailure,
        MacosOperation::Activate,
        RecoveryAction::CancelAndReap,
        "macOS resource sampling could not establish a complete observation",
    )
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "inventoried libproc read-only process-group resource observation boundary"
)]
mod native {
    use std::{
        ffi::{c_int, c_void},
        mem::{MaybeUninit, size_of_val},
    };

    use peritus_process::ProcessTreeIdentity;

    use super::{ResourceUsage, sample_error};
    use crate::MacosError;

    const MAX_GROUP_PROCESSES: usize = 16_384;
    const MAX_DESCRIPTOR_BYTES: usize = 512 * 1_024;

    pub(super) fn controls_available() -> bool {
        let mut major = 0;
        let mut minor = 0;
        // SAFETY: both pointers reference initialized writable integers and libproc transfers no
        // ownership. A zero result proves that the process-observation library is callable.
        if unsafe { libc::proc_libversion(&raw mut major, &raw mut minor) } != 0 || major < 1 {
            return false;
        }
        [libc::RLIMIT_CPU, libc::RLIMIT_AS, libc::RLIMIT_NOFILE].into_iter().all(|resource| {
            let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            // SAFETY: `limit` is writable initialized storage and `resource` is from the
            // closed macOS RLIMIT constant set above.
            (unsafe { libc::getrlimit(resource, &raw mut limit) }) == 0
        })
    }

    pub(super) fn sample_process_group(
        tree: ProcessTreeIdentity,
    ) -> Result<ResourceUsage, MacosError> {
        let group = tree.process_group().ok_or_else(sample_error)?;
        let mut pids = vec![0_i32; MAX_GROUP_PROCESSES];
        let buffer_bytes =
            c_int::try_from(size_of_val(pids.as_slice())).map_err(|_| sample_error())?;
        // SAFETY: `pids` is writable for `buffer_bytes`; the selector requests only process IDs
        // belonging to the exact C2-owned process group and transfers no ownership.
        let count = unsafe {
            libc::proc_listpgrppids(
                group.cast_signed(),
                pids.as_mut_ptr().cast::<c_void>(),
                buffer_bytes,
            )
        };
        if count < 0 {
            return Err(sample_error());
        }
        if count == 0 {
            return Ok(ResourceUsage::default());
        }
        let count = usize::try_from(count).map_err(|_| sample_error())?;
        if count >= pids.len() {
            return Err(sample_error());
        }
        pids.truncate(count);
        pids.retain(|pid| *pid > 0);
        pids.sort_unstable();
        pids.dedup();
        let mut usage = ResourceUsage {
            processes: u64::try_from(pids.len()).unwrap_or(u64::MAX),
            ..ResourceUsage::default()
        };
        let mut descriptor_buffer = vec![0_u8; MAX_DESCRIPTOR_BYTES];
        for pid in pids {
            let mut info = MaybeUninit::<libc::rusage_info_v2>::uninit();
            // SAFETY: `info` is correctly laid out writable V2 storage and the PID came from the
            // immediately preceding exact process-group enumeration.
            let observed = unsafe {
                libc::proc_pid_rusage(
                    pid,
                    libc::RUSAGE_INFO_V2,
                    info.as_mut_ptr().cast::<libc::rusage_info_t>(),
                )
            };
            if observed != 0 {
                continue;
            }
            // SAFETY: a zero return from `proc_pid_rusage` initialized the complete V2 record.
            let info = unsafe { info.assume_init() };
            usage.cpu_nanos = usage
                .cpu_nanos
                .saturating_add(info.ri_user_time)
                .saturating_add(info.ri_system_time);
            if u32::try_from(pid).ok() == Some(tree.root_pid()) {
                // Root child time retains completed descendants that could otherwise disappear
                // between supervisor polls. Any overlap is conservative and fails closed.
                usage.cpu_nanos = usage
                    .cpu_nanos
                    .saturating_add(info.ri_child_user_time)
                    .saturating_add(info.ri_child_system_time);
            }
            usage.memory_bytes = usage.memory_bytes.saturating_add(info.ri_resident_size);

            // SAFETY: the reusable byte buffer is writable for its exact declared length; the
            // selector is read-only and argument zero is required for PROC_PIDLISTFDS.
            let descriptor_bytes = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDLISTFDS,
                    0,
                    descriptor_buffer.as_mut_ptr().cast::<c_void>(),
                    i32::try_from(descriptor_buffer.len()).map_err(|_| sample_error())?,
                )
            };
            if descriptor_bytes > 0 {
                let descriptor_bytes =
                    usize::try_from(descriptor_bytes).map_err(|_| sample_error())?;
                if descriptor_bytes >= descriptor_buffer.len()
                    || !descriptor_bytes
                        .is_multiple_of(usize::try_from(libc::PROC_PIDLISTFD_SIZE).unwrap_or(1))
                {
                    return Err(sample_error());
                }
                usage.open_handles = usage.open_handles.saturating_add(
                    u64::try_from(
                        descriptor_bytes / usize::try_from(libc::PROC_PIDLISTFD_SIZE).unwrap_or(1),
                    )
                    .unwrap_or(u64::MAX),
                );
            }
        }
        Ok(usage)
    }
}

#[cfg(test)]
mod tests {
    use peritus_sandbox::SandboxResourceKind;

    use super::{ResourceUsage, exceeds};
    use crate::{EnforcementLevel, ResourceControl, ResourceControlPlan};

    #[test]
    fn resource_ceiling_comparison_is_exact_and_dimension_complete() {
        let controls = ResourceControlPlan::from_controls([
            ResourceControl::new(SandboxResourceKind::WallTime, 100, EnforcementLevel::Supervisor),
            ResourceControl::new(SandboxResourceKind::CpuTime, 100, EnforcementLevel::Supervisor),
            ResourceControl::new(SandboxResourceKind::Memory, 100, EnforcementLevel::Supervisor),
            ResourceControl::new(SandboxResourceKind::Disk, 100, EnforcementLevel::Supervisor),
            ResourceControl::new(SandboxResourceKind::Output, 100, EnforcementLevel::Supervisor),
            ResourceControl::new(
                SandboxResourceKind::OpenHandles,
                100,
                EnforcementLevel::Supervisor,
            ),
            ResourceControl::new(SandboxResourceKind::Processes, 100, EnforcementLevel::Supervisor),
            ResourceControl::new(
                SandboxResourceKind::Concurrency,
                100,
                EnforcementLevel::Supervisor,
            ),
        ]);
        let at_limit = ResourceUsage {
            cpu_nanos: controls.control(SandboxResourceKind::CpuTime).ceiling() * 1_000_000,
            memory_bytes: controls.control(SandboxResourceKind::Memory).ceiling(),
            disk_bytes: controls.control(SandboxResourceKind::Disk).ceiling(),
            open_handles: controls.control(SandboxResourceKind::OpenHandles).ceiling(),
            processes: controls.control(SandboxResourceKind::Processes).ceiling(),
        };
        assert!(!exceeds(&at_limit, &controls));
        assert!(exceeds(
            &ResourceUsage { processes: at_limit.processes + 1, ..at_limit },
            &controls,
        ));
    }
}
