//! Dimension-specific Linux resource projection and helper rlimits.

use crate::{EnforcementLevel, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
use peritus_sandbox::{CheckedSandboxPlan, ResourceLimits, SandboxResourceKind};

/// Complete native and supervisor resource projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourcePlan {
    wall_millis: u64,
    cpu_millis: u64,
    memory_bytes: u64,
    disk_bytes: u64,
    output_bytes: u64,
    open_handles: u64,
    processes: u64,
    concurrency: u64,
}

impl ResourcePlan {
    /// Projects every C2 resource dimension without broadening a ceiling.
    #[must_use]
    pub const fn from_limits(limits: &ResourceLimits) -> Self {
        Self {
            wall_millis: limits.limit(SandboxResourceKind::WallTime).get(),
            cpu_millis: limits.limit(SandboxResourceKind::CpuTime).get(),
            memory_bytes: limits.limit(SandboxResourceKind::Memory).get(),
            disk_bytes: limits.limit(SandboxResourceKind::Disk).get(),
            output_bytes: limits.limit(SandboxResourceKind::Output).get(),
            open_handles: limits.limit(SandboxResourceKind::OpenHandles).get(),
            processes: limits.limit(SandboxResourceKind::Processes).get(),
            concurrency: limits.limit(SandboxResourceKind::Concurrency).get(),
        }
    }
    /// Projects resource ceilings and narrows process count to the checked process contract.
    #[must_use]
    pub fn from_sandbox(plan: &CheckedSandboxPlan) -> Self {
        let mut resources = Self::from_limits(plan.requirements().resources());
        resources.processes =
            resources.processes.min(u64::from(plan.contract().process().maximum_processes()));
        resources
    }
    /// Wall-time ceiling in milliseconds.
    #[must_use]
    pub const fn wall_millis(self) -> u64 {
        self.wall_millis
    }
    /// CPU-time ceiling in milliseconds.
    #[must_use]
    pub const fn cpu_millis(self) -> u64 {
        self.cpu_millis
    }
    /// Address-space/cgroup memory ceiling in bytes.
    #[must_use]
    pub const fn memory_bytes(self) -> u64 {
        self.memory_bytes
    }
    /// File-size safeguard in bytes; aggregate disk remains supervisor-enforced.
    #[must_use]
    pub const fn disk_bytes(self) -> u64 {
        self.disk_bytes
    }
    /// C2-owned output ceiling in bytes.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }
    /// Open-descriptor ceiling.
    #[must_use]
    pub const fn open_handles(self) -> u64 {
        self.open_handles
    }
    /// Complete process-count ceiling.
    #[must_use]
    pub const fn processes(self) -> u64 {
        self.processes
    }
    /// C2 execution-slot ceiling.
    #[must_use]
    pub const fn concurrency(self) -> u64 {
        self.concurrency
    }
    /// Truthful enforcement class for one dimension.
    #[must_use]
    pub const fn enforcement(self, kind: SandboxResourceKind) -> EnforcementLevel {
        match kind {
            SandboxResourceKind::Memory
            | SandboxResourceKind::OpenHandles
            | SandboxResourceKind::Processes => EnforcementLevel::Hard,
            SandboxResourceKind::WallTime
            | SandboxResourceKind::CpuTime
            | SandboxResourceKind::Disk
            | SandboxResourceKind::Output
            | SandboxResourceKind::Concurrency => EnforcementLevel::Supervisor,
        }
    }

    pub(crate) fn encode(self, bytes: &mut Vec<u8>) {
        for value in [
            self.wall_millis,
            self.cpu_millis,
            self.memory_bytes,
            self.disk_bytes,
            self.output_bytes,
            self.open_handles,
            self.processes,
            self.concurrency,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }

    pub(crate) fn decode(reader: &mut crate::canonical::Reader<'_>) -> Result<Self, LinuxError> {
        let plan = Self {
            wall_millis: reader.u64()?,
            cpu_millis: reader.u64()?,
            memory_bytes: reader.u64()?,
            disk_bytes: reader.u64()?,
            output_bytes: reader.u64()?,
            open_handles: reader.u64()?,
            processes: reader.u64()?,
            concurrency: reader.u64()?,
        };
        if [
            plan.wall_millis,
            plan.cpu_millis,
            plan.memory_bytes,
            plan.disk_bytes,
            plan.output_bytes,
            plan.open_handles,
            plan.processes,
            plan.concurrency,
        ]
        .contains(&0)
        {
            return Err(LinuxError::new(
                LinuxErrorKind::Resource,
                LinuxOperation::Manifest,
                LinuxRecovery::CorrectRequest,
                "helper resource limits must be nonzero",
            ));
        }
        Ok(plan)
    }
}
