//! Abrupt loss of an owned virtual machine's volatile process state.

use std::process::{Child, Command, ExitStatus};

pub(super) struct PowerCut {
    terminated_pid: u32,
    restarted_pid: u32,
    exit: ExitStatus,
}

impl PowerCut {
    pub(super) fn observation(&self) -> String {
        format!(
            "terminated_pid:{},restarted_pid:{},exit:{}",
            self.terminated_pid,
            self.restarted_pid,
            self.exit.to_string().replace(' ', "_")
        )
    }
}

pub(super) fn restart(
    child: &mut Child,
    command: &mut Command,
) -> Result<PowerCut, Box<dyn std::error::Error>> {
    if child.try_wait()?.is_some() {
        return Err("cannot inject power loss into an exited guest".into());
    }
    let terminated_pid = child.id();
    // Do not ask the guest or QEMU to shut down or flush. Kill only the process we own,
    // then reap it before reopening its existing disk overlay in a new QEMU process.
    child.kill()?;
    let exit = child.wait()?;
    require_forced_exit(exit)?;
    *child = command.spawn()?;
    Ok(PowerCut { terminated_pid, restarted_pid: child.id(), exit })
}

fn require_forced_exit(status: ExitStatus) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if status.signal() != Some(9) {
            return Err(format!("guest did not exit from the injected SIGKILL: {status}").into());
        }
    }
    #[cfg(not(unix))]
    if status.success() {
        return Err("guest exited successfully instead of the injected termination".into());
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "power_tests.rs"]
mod tests;
