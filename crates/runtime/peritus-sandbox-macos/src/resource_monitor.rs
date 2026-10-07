//! Bounded macOS process-group and workspace resource sampling.

#[cfg(any(target_os = "macos", test))]
mod pids;

use std::{
    collections::BTreeMap,
    fs::ReadDir,
    path::{Path, PathBuf},
};

use peritus_sandbox::SandboxResourceKind;
use peritus_process::ProcessTreeIdentity;

use crate::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction, ResourceControlPlan};

#[cfg(target_os = "macos")]
const DISK_ENTRIES_PER_POLL: usize = 4_096;
#[cfg(target_os = "macos")]
pub(crate) fn native_controls_available() -> bool {
    native::controls_available()
}

pub(crate) struct ResourceMonitor {
    workspace: PathBuf,
    disk_selected: bool,
    #[cfg(target_os = "macos")]
    disk: Option<DiskTracker>,
    greatest: ResourceUsage,
}

impl ResourceMonitor {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "macOS construction performs a fallible selected disk baseline"
    )]
    pub(crate) fn new_cancellable(
        workspace: &Path,
        controls: &ResourceControlPlan,
        mut should_continue: impl FnMut() -> bool,
    ) -> Result<Self, MacosError> {
        let disk_selected = controls.control(SandboxResourceKind::Disk).is_selected();
        #[cfg(target_os = "macos")]
        let disk = disk_selected
            .then(|| DiskTracker::new(workspace, &mut should_continue))
            .transpose()?;
        #[cfg(not(target_os = "macos"))]
        let _ = &mut should_continue;
        Ok(Self {
            workspace: workspace.to_path_buf(),
            disk_selected,
            #[cfg(target_os = "macos")]
            disk,
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
            if let Measurement::Known(current) = native::sample_process_group(tree, controls)? {
                self.greatest.merge(current);
            }
            if let Some(disk) = &mut self.disk
                && let Measurement::Known(bytes) = disk.poll()
            {
                self.greatest.disk_bytes = self.greatest.disk_bytes.max(bytes);
            }
            Ok(exceeds(&self.greatest, controls))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (
                tree,
                controls,
                &self.workspace,
                self.disk_selected,
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

#[cfg(target_os = "macos")]
enum Measurement<T> {
    Known(T),
    Unknown,
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
    let cpu = controls.control(SandboxResourceKind::CpuTime);
    let memory = controls.control(SandboxResourceKind::Memory);
    let disk = controls.control(SandboxResourceKind::Disk);
    let handles = controls.control(SandboxResourceKind::OpenHandles);
    let processes = controls.control(SandboxResourceKind::Processes);
    let cpu_nanos = cpu.ceiling().saturating_mul(1_000_000);
    (cpu.is_selected() && usage.cpu_nanos > cpu_nanos)
        || (memory.is_selected() && usage.memory_bytes > memory.ceiling())
        || (disk.is_selected() && usage.disk_bytes > disk.ceiling())
        || (handles.is_selected() && usage.open_handles > handles.ceiling())
        || (processes.is_selected() && usage.processes > processes.ceiling())
}

#[cfg(target_os = "macos")]
struct DiskTracker {
    root: PathBuf,
    snapshot: BTreeMap<PathBuf, u64>,
    cumulative_growth: u64,
    scan: DiskScan,
}

#[cfg(target_os = "macos")]
impl DiskTracker {
    fn new(
        root: &Path,
        should_continue: &mut impl FnMut() -> bool,
    ) -> Result<Self, MacosError> {
        let mut scan = DiskScan::new(root);
        loop {
            if !should_continue() {
                return Err(MacosError::new(
                    MacosErrorKind::SupervisorFailure,
                    MacosOperation::Prepare,
                    RecoveryAction::CancelAndReap,
                    "selected disk baseline was cancelled by its owner",
                ));
            }
            if scan.advance(DISK_ENTRIES_PER_POLL) {
                break;
            }
        }
        if scan.uncertain {
            return Err(sample_error());
        }
        let snapshot = std::mem::take(&mut scan.observed);
        Ok(Self {
            root: root.to_path_buf(),
            snapshot,
            cumulative_growth: 0,
            scan: DiskScan::new(root),
        })
    }

    fn poll(&mut self) -> Measurement<u64> {
        if !self.scan.advance(DISK_ENTRIES_PER_POLL) {
            return Measurement::Unknown;
        }
        let uncertain = self.scan.uncertain;
        for (path, bytes) in &self.scan.observed {
            let previous = self.snapshot.get(path).copied().unwrap_or(0);
            self.cumulative_growth =
                self.cumulative_growth.saturating_add(bytes.saturating_sub(previous));
            self.snapshot.insert(path.clone(), *bytes);
        }
        if !uncertain {
            self.snapshot.retain(|path, _| self.scan.observed.contains_key(path));
        }
        self.scan = DiskScan::new(&self.root);
        if uncertain {
            Measurement::Unknown
        } else {
            Measurement::Known(self.cumulative_growth)
        }
    }
}

#[cfg(target_os = "macos")]
struct DiskScan {
    pending: Vec<PathBuf>,
    current: Option<ReadDir>,
    observed: BTreeMap<PathBuf, u64>,
    uncertain: bool,
}

#[cfg(target_os = "macos")]
impl DiskScan {
    fn new(root: &Path) -> Self {
        Self {
            pending: vec![root.to_path_buf()],
            current: None,
            observed: BTreeMap::new(),
            uncertain: false,
        }
    }

    fn advance(&mut self, mut remaining: usize) -> bool {
        while remaining != 0 {
            if self.current.is_none() {
                let Some(directory) = self.pending.pop() else {
                    return true;
                };
                match std::fs::read_dir(directory) {
                    Ok(entries) => self.current = Some(entries),
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => {
                        self.uncertain = true;
                        continue;
                    }
                }
            }
            let entry = self.current.as_mut().and_then(Iterator::next);
            let Some(entry) = entry else {
                self.current = None;
                continue;
            };
            remaining = remaining.saturating_sub(1);
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    self.uncertain = true;
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    self.uncertain = true;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                self.pending.push(path);
            } else if metadata.is_file() {
                self.observed.insert(path, metadata.len());
            }
        }
        false
    }
}

#[cfg(any(target_os = "macos", test))]
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
    use std::{ffi::c_void, mem::MaybeUninit};

    use peritus_process::ProcessTreeIdentity;

    use super::{Measurement, ResourceUsage, sample_error};
    use crate::MacosError;

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
        controls: &crate::ResourceControlPlan,
    ) -> Result<Measurement<ResourceUsage>, MacosError> {
        let cpu_selected = controls.control(crate::SandboxResourceKind::CpuTime).is_selected();
        let memory_selected = controls.control(crate::SandboxResourceKind::Memory).is_selected();
        let handles_selected = controls
            .control(crate::SandboxResourceKind::OpenHandles)
            .is_selected();
        let processes_selected = controls
            .control(crate::SandboxResourceKind::Processes)
            .is_selected();
        if !(cpu_selected || memory_selected || handles_selected || processes_selected) {
            return Ok(Measurement::Known(ResourceUsage::default()));
        }
        let group = tree.process_group().ok_or_else(sample_error)?;
        let super::pids::PidEnumeration::Known(mut pids) =
            super::pids::enumerate_all(group.cast_signed())?
        else {
            return Ok(Measurement::Unknown);
        };
        pids.retain(|pid| *pid > 0);
        pids.sort_unstable();
        pids.dedup();
        if pids.is_empty() {
            return Ok(Measurement::Known(ResourceUsage::default()));
        }
        let mut usage = ResourceUsage {
            processes: if processes_selected {
                u64::try_from(pids.len()).unwrap_or(u64::MAX)
            } else {
                0
            },
            ..ResourceUsage::default()
        };
        let mut live_cpu_nanos = 0_u64;
        let mut root_lifetime_cpu_nanos = 0_u64;
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
                return Ok(Measurement::Unknown);
            }
            // SAFETY: a zero return from `proc_pid_rusage` initialized the complete V2 record.
            let info = unsafe { info.assume_init() };
            if cpu_selected {
                let own_cpu = info
                    .ri_user_time
                    .saturating_add(info.ri_system_time);
                live_cpu_nanos = live_cpu_nanos.saturating_add(own_cpu);
                if u32::try_from(pid).ok() == Some(tree.root_pid()) {
                    root_lifetime_cpu_nanos = own_cpu
                        .saturating_add(info.ri_child_user_time)
                        .saturating_add(info.ri_child_system_time);
                }
            }
            if memory_selected {
                usage.memory_bytes = usage.memory_bytes.saturating_add(info.ri_resident_size);
            }

            if handles_selected {
                let Measurement::Known(handles) = sample_open_handles(pid)? else {
                    return Ok(Measurement::Unknown);
                };
                usage.open_handles = usage
                    .open_handles
                    .saturating_add(handles);
            }
        }
        if cpu_selected {
            // The root's child counters cover completed descendants; the live sum covers current
            // group members. Taking the larger lifetime view avoids counting live descendants twice.
            usage.cpu_nanos = live_cpu_nanos.max(root_lifetime_cpu_nanos);
        }
        Ok(Measurement::Known(usage))
    }

    fn sample_open_handles(pid: i32) -> Result<Measurement<u64>, MacosError> {
        let descriptor_size = usize::try_from(libc::PROC_PIDLISTFD_SIZE).map_err(|_| sample_error())?;
        let mut descriptor_buffer = vec![0_u8; descriptor_size.saturating_mul(64)];
        loop {
            // SAFETY: the resizable byte buffer is writable for its exact checked length; the
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
            if descriptor_bytes <= 0 {
                return Ok(Measurement::Unknown);
            }
            let descriptor_bytes = usize::try_from(descriptor_bytes).map_err(|_| sample_error())?;
            if descriptor_bytes > descriptor_buffer.len() {
                return Ok(Measurement::Unknown);
            }
            if descriptor_bytes == descriptor_buffer.len() {
                let next = descriptor_buffer.len().checked_mul(2).ok_or_else(sample_error)?;
                descriptor_buffer.resize(next, 0);
                continue;
            }
            if !descriptor_bytes.is_multiple_of(descriptor_size) {
                return Ok(Measurement::Unknown);
            }
            return Ok(Measurement::Known(
                u64::try_from(descriptor_bytes / descriptor_size).unwrap_or(u64::MAX),
            ));
        }
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
