//! Safe helper rlimit installation.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ResourcePlan};
use nix::sys::resource::{Resource, setrlimit};

pub(super) fn install(plan: ResourcePlan) -> Result<(), LinuxError> {
    let mut limits = vec![
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_FSIZE, plan.disk_bytes()),
        (Resource::RLIMIT_NOFILE, plan.open_handles()),
        (Resource::RLIMIT_NPROC, plan.processes()),
    ];
    // The prepared cgroup's `memory.max` is the authoritative resident-memory boundary. Do not
    // also project that byte ceiling onto RLIMIT_AS: current JavaScript engines and browsers
    // reserve large sparse virtual address cages which consume little resident memory, and an
    // address-space limit makes otherwise bounded processes abort during allocator startup.
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
