//! Production native process-probe recovery behavior.

use peritus_process::{
    ErrorCode, NativeProcessProbe, ProbeObservation, ProcessOperation, ProcessProbe,
    ProcessTreeIdentity, RecoveryClass,
};

#[test]
fn incomplete_birth_identity_is_never_live_or_terminable() {
    let identity = ProcessTreeIdentity::new(std::process::id(), None, None, false);
    let mut probe = NativeProcessProbe::new();
    assert_eq!(
        probe.observe(identity).expect("bounded observation"),
        ProbeObservation::Unverifiable
    );
    let error =
        probe.terminate(identity).expect_err("unverifiable identity must not be terminated");
    assert_eq!(error.code(), ErrorCode::Indeterminate);
    assert_eq!(error.operation(), ProcessOperation::Reconcile);
    assert_eq!(error.recovery(), RecoveryClass::ReopenAndReconcile);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_probe_matches_birth_token_and_terminates_only_the_exact_isolated_group() {
    use std::{os::unix::process::CommandExt, process::Command};

    let mut command = Command::new("sleep");
    command.arg("30").process_group(0);
    let child = command.spawn().expect("isolated probe fixture");
    let mut child = ChildGuard::new(child);
    let pid = child.id();
    let (process_group, start_token) = linux_stat(pid).expect("live fixture identity");
    assert_eq!(process_group, pid);

    let identity = ProcessTreeIdentity::new(pid, Some(start_token), Some(process_group), true);
    let changed =
        ProcessTreeIdentity::new(pid, Some(start_token.wrapping_add(1)), Some(process_group), true);
    let mut probe = NativeProcessProbe::new();
    assert_eq!(probe.observe(identity).expect("exact observation"), ProbeObservation::ExactLive);
    assert_eq!(probe.observe(changed).expect("changed observation"), ProbeObservation::Mismatched);

    probe.terminate(identity).expect("exact group termination request");
    assert!(!child.wait().expect("terminated child remains waitable").success());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_probe_reports_a_missing_pid_as_exactly_absent() {
    let absent_pid = u32::try_from(i32::MAX).expect("positive platform PID maximum");
    let identity = ProcessTreeIdentity::new(absent_pid, Some(1), Some(absent_pid), true);
    let mut probe = NativeProcessProbe::new();
    assert_eq!(probe.observe(identity).expect("absent observation"), ProbeObservation::ExactAbsent);
}

#[cfg(target_os = "linux")]
fn linux_stat(pid: u32) -> Option<(u32, u64)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let after_name = close.checked_add(1)?;
    let mut fields = text.get(after_name..)?.split_ascii_whitespace();
    let _state = fields.next()?;
    let _parent_pid = fields.next()?;
    let process_group = fields.next()?.parse().ok()?;
    let start_token = fields.nth(16)?.parse().ok()?;
    Some((process_group, start_token))
}

#[cfg(target_os = "linux")]
struct ChildGuard(Option<std::process::Child>);

#[cfg(target_os = "linux")]
impl ChildGuard {
    fn new(child: std::process::Child) -> Self {
        Self(Some(child))
    }

    fn id(&self) -> u32 {
        self.0.as_ref().expect("guard retains child").id()
    }

    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.0.as_mut().expect("guard retains child").wait()
    }
}

#[cfg(target_os = "linux")]
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
