//! Checked resource budgets and reference-backend accounting.

use crate::{SandboxError, SandboxErrorKind, SandboxOperation};
use peritus_types::ResourceQuantity;

/// Resource dimensions enforced by sandbox backends.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxResourceKind {
    /// Elapsed wall time in milliseconds.
    WallTime,
    /// CPU time in milliseconds.
    CpuTime,
    /// Resident/committed memory in bytes.
    Memory,
    /// Filesystem usage in bytes.
    Disk,
    /// Captured output in bytes.
    Output,
    /// Simultaneously open handles.
    OpenHandles,
    /// Simultaneous owned processes.
    Processes,
    /// Simultaneous execution slots.
    Concurrency,
}

impl SandboxResourceKind {
    pub(crate) const ALL: [Self; 8] = [
        Self::WallTime,
        Self::CpuTime,
        Self::Memory,
        Self::Disk,
        Self::Output,
        Self::OpenHandles,
        Self::Processes,
        Self::Concurrency,
    ];

    const fn index(self) -> usize {
        match self {
            Self::WallTime => 0,
            Self::CpuTime => 1,
            Self::Memory => 2,
            Self::Disk => 3,
            Self::Output => 4,
            Self::OpenHandles => 5,
            Self::Processes => 6,
            Self::Concurrency => 7,
        }
    }
}

/// Complete nonzero upper bounds for every sandbox resource dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    wall_time: Option<ResourceQuantity>,
    cpu_time: Option<ResourceQuantity>,
    output: Option<ResourceQuantity>,
    values: [ResourceQuantity; 6],
}

impl ResourceLimits {
    /// Creates a complete resource budget.
    ///
    /// # Errors
    /// Rejects a zero upper bound in any dimension.
    #[allow(clippy::too_many_arguments, reason = "one typed value per closed resource dimension")]
    pub fn new(
        wall_time: ResourceQuantity,
        cpu_time: ResourceQuantity,
        memory: ResourceQuantity,
        disk: ResourceQuantity,
        output: ResourceQuantity,
        open_handles: ResourceQuantity,
        processes: ResourceQuantity,
        concurrency: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        Self::with_optional_wall_and_output(
            Some(wall_time),
            Some(output),
            cpu_time,
            memory,
            disk,
            open_handles,
            processes,
            concurrency,
        )
    }

    /// Creates a complete resource budget with an optional wall-time ceiling.
    ///
    /// # Errors
    /// Rejects a present zero ceiling or a zero required dimension.
    #[allow(clippy::too_many_arguments, reason = "one typed value per closed resource dimension")]
    pub fn with_optional_wall_time(
        wall_time: Option<ResourceQuantity>,
        cpu_time: ResourceQuantity,
        memory: ResourceQuantity,
        disk: ResourceQuantity,
        output: ResourceQuantity,
        open_handles: ResourceQuantity,
        processes: ResourceQuantity,
        concurrency: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        Self::with_optional_wall_and_output(
            wall_time,
            Some(output),
            cpu_time,
            memory,
            disk,
            open_handles,
            processes,
            concurrency,
        )
    }

    /// Creates a complete resource budget with optional wall-time and aggregate-output ceilings.
    ///
    /// # Errors
    /// Rejects a present zero ceiling or a zero required dimension.
    #[allow(clippy::too_many_arguments, reason = "one typed value per closed resource dimension")]
    pub fn with_optional_wall_and_output(
        wall_time: Option<ResourceQuantity>,
        output: Option<ResourceQuantity>,
        cpu_time: ResourceQuantity,
        memory: ResourceQuantity,
        disk: ResourceQuantity,
        open_handles: ResourceQuantity,
        processes: ResourceQuantity,
        concurrency: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        Self::with_optional_wall_output_cpu(
            wall_time,
            output,
            Some(cpu_time),
            memory,
            disk,
            open_handles,
            processes,
            concurrency,
        )
    }

    /// Creates a complete budget with optional wall, CPU-time, and aggregate-output ceilings.
    ///
    /// # Errors
    /// Rejects zero values for explicitly supplied quotas or any required resource limit.
    #[allow(clippy::too_many_arguments, reason = "one typed value per closed resource dimension")]
    pub fn with_optional_wall_output_cpu(
        wall_time: Option<ResourceQuantity>,
        output: Option<ResourceQuantity>,
        cpu_time: Option<ResourceQuantity>,
        memory: ResourceQuantity,
        disk: ResourceQuantity,
        open_handles: ResourceQuantity,
        processes: ResourceQuantity,
        concurrency: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        let values = [memory, disk, open_handles, processes, concurrency];
        if wall_time.is_some_and(|value| value.get() == 0)
            || cpu_time.is_some_and(|value| value.get() == 0)
            || output.is_some_and(|value| value.get() == 0)
            || values.iter().any(|value| value.get() == 0)
        {
            return Err(crate::error::invalid("resource limits must be nonzero"));
        }
        let [memory, disk, open_handles, processes, concurrency] = values;
        Ok(Self {
            wall_time,
            cpu_time,
            output,
            values: [
                cpu_time.unwrap_or(ResourceQuantity::zero()),
                memory,
                disk,
                open_handles,
                processes,
                concurrency,
            ],
        })
    }

    /// Returns the bound for one dimension.
    #[must_use]
    pub fn limit(&self, kind: SandboxResourceKind) -> ResourceQuantity {
        match kind {
            SandboxResourceKind::WallTime => self.wall_time.unwrap_or(ResourceQuantity::zero()),
            SandboxResourceKind::CpuTime => self.cpu_time.unwrap_or(ResourceQuantity::zero()),
            SandboxResourceKind::Output => self.output.unwrap_or(ResourceQuantity::zero()),
            _ => {
                self.values[match kind {
                    SandboxResourceKind::CpuTime => 0,
                    SandboxResourceKind::Memory => 1,
                    SandboxResourceKind::Disk => 2,
                    SandboxResourceKind::OpenHandles => 3,
                    SandboxResourceKind::Processes => 4,
                    SandboxResourceKind::Concurrency => 5,
                    SandboxResourceKind::WallTime | SandboxResourceKind::Output => unreachable!(),
                }]
            }
        }
    }

    /// Returns the optional wall-time ceiling. Zero is never used as its absence marker.
    #[must_use]
    pub const fn wall_time_limit(&self) -> Option<ResourceQuantity> {
        self.wall_time
    }

    /// Returns the optional aggregate-output ceiling.
    #[must_use]
    pub const fn output_limit(&self) -> Option<ResourceQuantity> {
        self.output
    }

    /// Returns the optional CPU-time ceiling.
    #[must_use]
    pub const fn cpu_time_limit(&self) -> Option<ResourceQuantity> {
        self.cpu_time
    }

    /// Reports the first dimension whose requested bound exceeds this contract.
    #[must_use]
    pub fn first_exceeded_by(&self, requested: &Self) -> Option<SandboxResourceKind> {
        SandboxResourceKind::ALL.into_iter().find(|kind| {
            match (requested.optional_limit(*kind), self.optional_limit(*kind)) {
                (None, Some(_)) => true,
                (Some(requested), Some(allowed)) => requested > allowed,
                _ => false,
            }
        })
    }

    fn optional_limit(&self, kind: SandboxResourceKind) -> Option<ResourceQuantity> {
        match kind {
            SandboxResourceKind::WallTime => self.wall_time,
            SandboxResourceKind::CpuTime => self.cpu_time,
            SandboxResourceKind::Output => self.output,
            _ => Some(self.limit(kind)),
        }
    }
}

/// Accumulated reference-backend usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUsage {
    values: [ResourceQuantity; 8],
}

impl ResourceUsage {
    /// Returns zero usage in every dimension.
    #[must_use]
    pub const fn zero() -> Self {
        Self { values: [ResourceQuantity::zero(); 8] }
    }
    /// Returns usage for one dimension.
    #[must_use]
    pub const fn get(&self, kind: SandboxResourceKind) -> ResourceQuantity {
        self.values[kind.index()]
    }

    /// Adds usage, rejecting arithmetic overflow or a budget violation.
    ///
    /// # Errors
    /// Returns `ResourceLimit` when the sum overflows or exceeds the limit.
    pub fn charge(
        &mut self,
        kind: SandboxResourceKind,
        quantity: ResourceQuantity,
        limits: &ResourceLimits,
    ) -> Result<(), SandboxError> {
        if limits.optional_limit(kind).is_some_and(|limit| {
            !crate::verified::resource_charge_allowed(
                self.get(kind).get(),
                quantity.get(),
                limit.get(),
            )
        }) {
            return Err(SandboxError::new(
                SandboxErrorKind::ResourceLimit,
                SandboxOperation::Account,
                crate::RecoveryClass::CancelAndRelease,
                "resource limit exceeded",
            ));
        }
        let sum = self.get(kind).checked_add(quantity).map_err(|_| {
            SandboxError::new(
                SandboxErrorKind::ResourceLimit,
                SandboxOperation::Account,
                crate::RecoveryClass::CancelAndRelease,
                "resource usage overflow",
            )
        })?;
        self.values[kind.index()] = sum;
        Ok(())
    }
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_wall_cpu_and_output_have_no_synthetic_ceilings() {
        let one = ResourceQuantity::new(1);
        let limits = ResourceLimits::with_optional_wall_output_cpu(
            None, None, None, one, one, one, one, one,
        )
        .expect("all finite resource dimensions are positive");
        assert_eq!(limits.wall_time_limit(), None);
        assert_eq!(limits.cpu_time_limit(), None);
        assert_eq!(limits.output_limit(), None);
        let mut usage = ResourceUsage::zero();
        usage
            .charge(SandboxResourceKind::WallTime, ResourceQuantity::new(u64::MAX), &limits)
            .expect("wall time is not charged against an invented ceiling");
        usage
            .charge(SandboxResourceKind::Output, ResourceQuantity::new(u64::MAX), &limits)
            .expect("aggregate output is not charged against an invented ceiling");
        usage
            .charge(SandboxResourceKind::CpuTime, ResourceQuantity::new(u64::MAX), &limits)
            .expect("CPU time is not charged against an invented ceiling");
    }
}
