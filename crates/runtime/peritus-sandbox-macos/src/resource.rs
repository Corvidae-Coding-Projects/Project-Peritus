//! Dimension-specific macOS resource projection.

use peritus_sandbox::{CheckedSandboxPlan, SandboxResourceKind};

/// Native enforcement level for one resource dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EnforcementLevel {
    /// The helper installs a kernel resource limit before target execution.
    Hard,
    /// The process-owned C2 supervisor observes and terminates on violation.
    Supervisor,
    /// The current probe cannot enforce the dimension.
    Unsupported,
    /// Release or recovery could not prove continued enforcement.
    Incomplete,
}

impl EnforcementLevel {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Hard => 0,
            Self::Supervisor => 1,
            Self::Unsupported => 2,
            Self::Incomplete => 3,
        }
    }

    pub(crate) const fn from_ordinal(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Hard),
            1 => Some(Self::Supervisor),
            2 => Some(Self::Unsupported),
            3 => Some(Self::Incomplete),
            _ => None,
        }
    }
}

/// One resource ceiling and its exact enforcement owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceControl {
    kind: SandboxResourceKind,
    ceiling: u64,
    level: EnforcementLevel,
}

impl ResourceControl {
    /// Creates a dimension-specific control.
    #[must_use]
    pub const fn new(kind: SandboxResourceKind, ceiling: u64, level: EnforcementLevel) -> Self {
        Self { kind, ceiling, level }
    }

    /// Returns the controlled dimension.
    #[must_use]
    pub const fn kind(self) -> SandboxResourceKind {
        self.kind
    }

    /// Returns the exact checked ceiling.
    #[must_use]
    pub const fn ceiling(self) -> u64 {
        self.ceiling
    }

    /// Returns the enforcement owner.
    #[must_use]
    pub const fn level(self) -> EnforcementLevel {
        self.level
    }
}

/// Complete closed resource-control mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceControlPlan {
    controls: [ResourceControl; 8],
}

impl ResourceControlPlan {
    pub(crate) const fn from_controls(controls: [ResourceControl; 8]) -> Self {
        Self { controls }
    }

    /// Projects every checked resource dimension using the supplied probe result.
    #[must_use]
    pub fn from_checked_plan(plan: &CheckedSandboxPlan, levels: [EnforcementLevel; 8]) -> Self {
        let limits = plan.requirements().resources();
        let kinds = [
            SandboxResourceKind::WallTime,
            SandboxResourceKind::CpuTime,
            SandboxResourceKind::Memory,
            SandboxResourceKind::Disk,
            SandboxResourceKind::Output,
            SandboxResourceKind::OpenHandles,
            SandboxResourceKind::Processes,
            SandboxResourceKind::Concurrency,
        ];
        let controls = std::array::from_fn(|index| {
            let kind = kinds[index];
            let ceiling = limits.limit(kind).get();
            let level = if kind == SandboxResourceKind::CpuTime
                && levels[index] == EnforcementLevel::Hard
                && !ceiling.is_multiple_of(1_000)
            {
                // RLIMIT_CPU has whole-second granularity. C2 must supervise subsecond or
                // fractional-second ceilings instead of overstating a rounded-up hard limit.
                EnforcementLevel::Supervisor
            } else {
                levels[index]
            };
            ResourceControl::new(kind, ceiling, level)
        });
        Self { controls }
    }

    /// Returns all dimensions in stable order.
    #[must_use]
    pub const fn controls(&self) -> &[ResourceControl; 8] {
        &self.controls
    }

    /// Finds one dimension.
    #[must_use]
    pub const fn control(&self, kind: SandboxResourceKind) -> ResourceControl {
        self.controls[resource_ordinal(kind) as usize]
    }

    /// Reports whether every dimension has an enforcement owner.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.controls.iter().all(|control| {
            matches!(control.level, EnforcementLevel::Hard | EnforcementLevel::Supervisor)
        })
    }
}

pub(crate) const fn resource_ordinal(kind: SandboxResourceKind) -> u8 {
    match kind {
        SandboxResourceKind::WallTime => 0,
        SandboxResourceKind::CpuTime => 1,
        SandboxResourceKind::Memory => 2,
        SandboxResourceKind::Disk => 3,
        SandboxResourceKind::Output => 4,
        SandboxResourceKind::OpenHandles => 5,
        SandboxResourceKind::Processes => 6,
        SandboxResourceKind::Concurrency => 7,
    }
}

pub(crate) const fn resource_from_ordinal(value: u8) -> Option<SandboxResourceKind> {
    match value {
        0 => Some(SandboxResourceKind::WallTime),
        1 => Some(SandboxResourceKind::CpuTime),
        2 => Some(SandboxResourceKind::Memory),
        3 => Some(SandboxResourceKind::Disk),
        4 => Some(SandboxResourceKind::Output),
        5 => Some(SandboxResourceKind::OpenHandles),
        6 => Some(SandboxResourceKind::Processes),
        7 => Some(SandboxResourceKind::Concurrency),
        _ => None,
    }
}
