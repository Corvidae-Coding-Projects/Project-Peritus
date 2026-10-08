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

/// Explicit upper bounds for sandbox resource dimensions.
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

    /// Creates resource bounds without imposing wall or CPU time limits.
    ///
    /// # Errors
    /// Rejects zero bounds for every non-time dimension.
    #[allow(clippy::too_many_arguments, reason = "one typed value per closed resource dimension")]
    pub fn with_optional_time(
        wall_time: Option<ResourceQuantity>,
        cpu_time: Option<ResourceQuantity>,
        memory: ResourceQuantity,
        disk: ResourceQuantity,
        output: ResourceQuantity,
        open_handles: ResourceQuantity,
        processes: ResourceQuantity,
        concurrency: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        Self::with_optional_limits(
            wall_time,
            cpu_time,
            Some(memory),
            Some(disk),
            Some(output),
            Some(open_handles),
            Some(processes),
            Some(concurrency),
        )
    }

    /// Creates explicit optional resource ceilings.
    ///
    /// A missing ceiling authorizes observation without inventing a cumulative maximum. Zero is
    /// reserved as the canonical representation of absence.
    ///
    /// # Errors
    /// Rejects a selected zero ceiling.
    #[allow(clippy::too_many_arguments, reason = "one typed value per closed resource dimension")]
    pub fn with_optional_limits(
        wall_time: Option<ResourceQuantity>,
        cpu_time: Option<ResourceQuantity>,
        memory: Option<ResourceQuantity>,
        disk: Option<ResourceQuantity>,
        output: Option<ResourceQuantity>,
        open_handles: Option<ResourceQuantity>,
        processes: Option<ResourceQuantity>,
        concurrency: Option<ResourceQuantity>,
    ) -> Result<Self, SandboxError> {
        let selected = [
            wall_time,
            cpu_time,
            memory,
            disk,
            output,
            open_handles,
            processes,
            concurrency,
        ];
        if selected.into_iter().flatten().any(|value| value.get() == 0) {
            return Err(crate::error::invalid("selected resource limits must be nonzero"));
        }
        Ok(Self { values: selected.map(|value| value.unwrap_or(ResourceQuantity::zero())) })
    }

    /// Returns an optional time bound; zero in the canonical wire denotes its absence.
    #[must_use]
    pub const fn time_limit(&self, kind: SandboxResourceKind) -> Option<ResourceQuantity> {
        self.selected_limit(kind)
    }

    /// Returns a selected ceiling, or `None` when that dimension is observed without a maximum.
    #[must_use]
    pub const fn selected_limit(
        &self,
        kind: SandboxResourceKind,
    ) -> Option<ResourceQuantity> {
        let value = self.limit(kind);
        if value.get() == 0 { None } else { Some(value) }
    }

    /// Returns the bound for one dimension.
    #[must_use]
    pub const fn limit(&self, kind: SandboxResourceKind) -> ResourceQuantity {
        self.values[kind.index()]
    }

    /// Reports the first dimension whose requested bound exceeds this contract.
    #[must_use]
    pub fn first_exceeded_by(&self, requested: &Self) -> Option<SandboxResourceKind> {
        SandboxResourceKind::ALL.into_iter().find(|kind| {
            self.selected_limit(*kind).is_some_and(|maximum| {
                requested.selected_limit(*kind).is_none_or(|value| value > maximum)
            })
        })
    }

    pub(crate) const fn uses_legacy_encoding(&self) -> bool {
        self.selected_limit(SandboxResourceKind::Memory).is_some()
            && self.selected_limit(SandboxResourceKind::Disk).is_some()
            && self.selected_limit(SandboxResourceKind::Output).is_some()
            && self.selected_limit(SandboxResourceKind::OpenHandles).is_some()
            && self.selected_limit(SandboxResourceKind::Processes).is_some()
            && self.selected_limit(SandboxResourceKind::Concurrency).is_some()
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
        if limits.selected_limit(kind).is_some_and(|maximum| {
            !crate::verified::resource_charge_allowed(
                self.get(kind).get(),
                quantity.get(),
                maximum.get(),
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
