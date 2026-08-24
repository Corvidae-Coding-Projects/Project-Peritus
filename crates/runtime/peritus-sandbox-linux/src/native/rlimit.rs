//! Safe helper rlimit installation.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ResourcePlan};
use nix::sys::resource::{Resource, setrlimit};

pub(super) fn install(plan: ResourcePlan) -> Result<(), LinuxError> {
    let cpu_seconds = plan.cpu_millis().div_ceil(1_000).max(1);
    for (resource, value) in [
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_CPU, cpu_seconds),
        (Resource::RLIMIT_AS, plan.memory_bytes()),
        (Resource::RLIMIT_FSIZE, plan.disk_bytes()),
        (Resource::RLIMIT_NOFILE, plan.open_handles()),
        (Resource::RLIMIT_NPROC, plan.processes()),
    ] {
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
