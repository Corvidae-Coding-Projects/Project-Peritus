//! Stable nonsensitive names for Windows resource diagnostics.

use peritus_sandbox::SandboxResourceKind;

use crate::EnforcementLevel;

/// Returns the stable resource dimension name for diagnostics.
#[must_use]
pub const fn resource_name(kind: SandboxResourceKind) -> &'static str {
    match kind {
        SandboxResourceKind::WallTime => "wall-time",
        SandboxResourceKind::CpuTime => "cpu-time",
        SandboxResourceKind::Memory => "memory",
        SandboxResourceKind::Disk => "disk",
        SandboxResourceKind::Output => "output",
        SandboxResourceKind::OpenHandles => "open-handles",
        SandboxResourceKind::Processes => "processes",
        SandboxResourceKind::Concurrency => "concurrency",
    }
}

/// Returns the stable enforcement level name for diagnostics.
#[must_use]
pub const fn enforcement_name(level: EnforcementLevel) -> &'static str {
    match level {
        EnforcementLevel::Hard => "hard",
        EnforcementLevel::Supervisor => "supervisor",
        EnforcementLevel::Unsupported => "unsupported",
        EnforcementLevel::Incomplete => "incomplete",
    }
}
