//! Process-ownership regressions; these are not VM power-loss qualification evidence.

use std::os::unix::process::ExitStatusExt as _;
use std::process::{Child, Command, ExitStatus, Stdio};

use super::{require_forced_exit, restart};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn sleeper() -> Command {
    let mut command = Command::new("sh");
    command
        .args(["-c", "exec sleep 60"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[test]
fn power_cut_reaps_the_exact_child_before_starting_its_replacement() {
    let mut command = sleeper();
    let mut child = OwnedChild(command.spawn().expect("start owned subject"));
    let previous = child.0.id();
    let observation = restart(&mut child.0, &mut command).expect("restart owned subject");
    assert_eq!(observation.terminated_pid, previous);
    assert_ne!(observation.restarted_pid, previous);
    assert_eq!(observation.restarted_pid, child.0.id());
    assert_eq!(observation.exit.signal(), Some(9));
    assert!(child.0.try_wait().expect("inspect replacement").is_none());
    assert!(observation.observation().contains("signal:_9"));
}

#[test]
fn an_already_exited_guest_cannot_supply_power_loss_evidence() {
    let mut command = sleeper();
    let mut child = OwnedChild(command.spawn().expect("start owned subject"));
    child.0.kill().expect("terminate before injection");
    child.0.wait().expect("reap before injection");
    let previous = child.0.id();
    assert!(restart(&mut child.0, &mut command).is_err());
    assert_eq!(child.0.id(), previous);
    assert!(child.0.try_wait().expect("inspect terminated child").is_some());
}

#[test]
fn failed_relaunch_does_not_leave_the_original_guest_running() {
    let mut child = OwnedChild(sleeper().spawn().expect("start owned subject"));
    let root = tempfile::tempdir().expect("own missing executable parent");
    let mut command = Command::new(root.path().join("missing-qemu"));
    assert!(restart(&mut child.0, &mut command).is_err());
    assert_eq!(child.0.try_wait().expect("inspect reaped guest").unwrap().signal(), Some(9));
}

#[test]
fn success_and_unrelated_signals_are_not_power_cut_observations() {
    assert!(require_forced_exit(ExitStatus::from_raw(0)).is_err());
    assert!(require_forced_exit(ExitStatus::from_raw(15)).is_err());
    assert!(require_forced_exit(ExitStatus::from_raw(9)).is_ok());
}
