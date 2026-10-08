//! Safe helper rlimit installation.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ResourcePlan};
use nix::sys::resource::{Resource, setrlimit};

pub(super) fn install(plan: ResourcePlan) -> Result<(), LinuxError> {
    let cpu_seconds = plan.cpu_millis().div_ceil(1_000).max(1);
    setrlimit(Resource::RLIMIT_CORE, 0, 0).map_err(|_| resource_error())?;
    for (resource, value) in [
        (Resource::RLIMIT_AS, plan.memory_bytes()),
        (Resource::RLIMIT_FSIZE, plan.disk_bytes()),
        (Resource::RLIMIT_NOFILE, plan.open_handles()),
        (Resource::RLIMIT_NPROC, plan.processes()),
    ] {
        if value != 0 {
            setrlimit(resource, value, value).map_err(|_| resource_error())?;
        }
    }
    if plan.cpu_millis() != 0 {
        setrlimit(Resource::RLIMIT_CPU, cpu_seconds, cpu_seconds)
            .map_err(|_| resource_error())?;
    }
    Ok(())
}

fn resource_error() -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Resource,
        LinuxOperation::Activate,
        LinuxRecovery::CancelAndReap,
        "helper could not install an exact selected rlimit",
    )
}
