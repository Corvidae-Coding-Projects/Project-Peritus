//! Platform-specific exact Git owner containment and termination.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::{
    Containment, Watchdog, activate_command, activate_owner, command_binding, complete,
    configure_command_runner, configure_git_child, configure_owner_command, drain_command,
    end_command, finish_watchdog, launcher_binding, owned_command_exited, start_watchdog,
    terminate, terminate_command, terminate_owned_command, validate_launcher_binding,
    validate_owned_command, validate_owner_binding, validate_watchdog, watchdog_terminate,
};
#[cfg(windows)]
pub(super) use windows::{
    Containment, Watchdog, activate_command, activate_owner, command_binding, complete,
    configure_command_runner, configure_git_child, configure_owner_command, drain_command,
    end_command, finish_watchdog, launcher_binding, owned_command_exited, start_watchdog,
    terminate, terminate_command, terminate_owned_command, validate_launcher_binding,
    validate_owned_command, validate_owner_binding, validate_watchdog, watchdog_terminate,
};
