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
    values: [ResourceQuantity; 8],
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
        let values =
            [wall_time, cpu_time, memory, disk, output, open_handles, processes, concurrency];
        if values.iter().any(|value| value.get() == 0) {
            return Err(crate::error::invalid("resource limits must be nonzero"));
        }
        Ok(Self { values })
    }

    /// Returns the bound for one dimension.
    #[must_use]
    pub const fn limit(&self, kind: SandboxResourceKind) -> ResourceQuantity {
        self.values[kind.index()]
    }

    /// Reports the first dimension whose requested bound exceeds this contract.
    #[must_use]
    pub fn first_exceeded_by(&self, requested: &Self) -> Option<SandboxResourceKind> {
        SandboxResourceKind::ALL.into_iter().find(|kind| requested.limit(*kind) > self.limit(*kind))
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
        if !crate::verified::resource_charge_allowed(
            self.get(kind).get(),
            quantity.get(),
            limits.limit(kind).get(),
        ) {
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
