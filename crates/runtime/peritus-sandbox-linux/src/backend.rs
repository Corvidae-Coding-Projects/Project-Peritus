//! Stable Linux backend identity and helper entry points.

use crate::KernelVersion;
#[cfg(target_os = "linux")]
use crate::{LinuxError, LinuxErrorKind};

/// Stable implementation name used in C2 descriptor selection.
pub const BACKEND_NAME: &str = "peritus-linux";
/// Native backend implementation version.
pub const BACKEND_VERSION: &str = "1";
/// Minimum supported Linux kernel release.
pub const MINIMUM_KERNEL: KernelVersion = KernelVersion::new(6, 6, 0);
/// Minimum supported Landlock ABI.
pub const MINIMUM_LANDLOCK_ABI: u8 = 3;

/// Runs the installed Linux helper protocol entry point.
///
/// This is exported solely for the crate-owned helper binary. It performs no authorization and
/// accepts only C2's bounded protected-channel protocol.
///
/// # Errors
/// Returns a typed helper or native enforcement failure before target execution.
#[cfg(target_os = "linux")]
pub fn run_linux_helper() -> Result<(), LinuxError> {
    crate::native::helper_main()
}

/// Returns the reserved helper exit category for a pre-target failure.
#[must_use]
#[cfg(target_os = "linux")]
pub const fn helper_exit_code(error: &LinuxError) -> i32 {
    match error.kind() {
        LinuxErrorKind::Helper | LinuxErrorKind::PreparationMismatch => 121,
        LinuxErrorKind::SandboxDenied | LinuxErrorKind::Filesystem | LinuxErrorKind::Resource => {
            120
        }
        _ => 122,
    }
}
