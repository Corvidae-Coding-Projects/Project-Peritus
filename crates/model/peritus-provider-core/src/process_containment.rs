//! Host process containment for credential-owning provider executables.

use tokio::process::Command;

/// Binds a provider child to the daemon process on hosts with a parent-death primitive.
///
/// Tokio's `kill_on_drop` covers ordinary cancellation. Linux parent-death signaling also covers
/// a daemon abort or `SIGKILL`, when Rust destructors cannot run.
#[cfg(target_os = "linux")]
#[allow(
    unsafe_code,
    reason = "prctl in the post-fork child is the narrow Linux boundary for daemon-death containment"
)]
pub fn configure(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;

    command.kill_on_drop(true);
    let daemon = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    command.as_std_mut().process_group(0);
    // SAFETY: the closure runs after fork and before exec. `prctl` and `getppid` are
    // async-signal-safe Linux syscalls, and the parent identity is a copied integer.
    unsafe {
        command.as_std_mut().pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != daemon {
                libc::_exit(125);
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
pub fn configure(command: &mut Command) {
    command.kill_on_drop(true);
}
