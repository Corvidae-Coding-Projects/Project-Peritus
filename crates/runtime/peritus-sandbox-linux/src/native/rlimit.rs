//! Safe helper rlimit installation.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ResourcePlan};
use nix::sys::resource::{Resource, setrlimit};

pub(super) fn install(plan: ResourcePlan) -> Result<(), LinuxError> {
    let mut limits = vec![
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_AS, plan.memory_bytes()),
        (Resource::RLIMIT_FSIZE, plan.disk_bytes()),
        (Resource::RLIMIT_NOFILE, plan.open_handles()),
        (Resource::RLIMIT_NPROC, plan.processes()),
    ];
    if let Some(cpu_millis) = plan.cpu_millis() {
        limits.push((Resource::RLIMIT_CPU, cpu_millis.div_ceil(1_000).max(1)));
    }
    for (resource, value) in limits {
        setrlimit(resource, value, value).map_err(|_| {
            LinuxError::new(
                LinuxErrorKind::Resource,
                LinuxOperation::Activate,
                LinuxRecovery::CancelAndReap,
                "helper could not install an exact rlimit",
            )
        })?;
    }
    Ok(())
}
