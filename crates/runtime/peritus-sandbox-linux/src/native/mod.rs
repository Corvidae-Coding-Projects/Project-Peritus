//! Linux-only safe native enforcement adapters.
//!
//! No unsafe code exists in this module. Kernel interfaces are reached through reviewed safe
//! wrappers (`landlock`, `seccompiler`, and `nix`) or explicit proc/cgroup files.

mod helper;
mod landlock_policy;
mod rlimit;
mod seccomp_policy;

pub fn helper_main() -> Result<(), crate::LinuxError> {
    helper::helper_main()
}
