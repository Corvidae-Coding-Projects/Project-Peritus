//! Hidden independent owner-death mode for exact daemon-launched process groups.

use std::{
    io::Read as _,
    process::ExitCode,
    time::Duration,
};

use peritus_process::{NativeProcessProbe, ProbeObservation, ProcessProbe, ProcessTreeIdentity};

const DISARM: u8 = 1;
const REOBSERVE_INTERVAL: Duration = Duration::from_millis(5);

pub(super) fn run(root: u32, start: u64, group: u32) -> ExitCode {
    if root == 0 || root != group {
        return ExitCode::FAILURE;
    }
    let mut signal = [0_u8; 1];
    loop {
        match std::io::stdin().read(&mut signal) {
            Ok(1) if signal[0] == DISARM => return ExitCode::SUCCESS,
            Ok(0 | 1) => break,
            Ok(_) => return ExitCode::FAILURE,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    terminate(root, start, group).map_or(ExitCode::FAILURE, |()| ExitCode::SUCCESS)
}

fn terminate(root: u32, start: u64, group: u32) -> Result<(), ()> {
    let identity = ProcessTreeIdentity::new(root, Some(start), Some(group), true);
    let mut probe = NativeProcessProbe::new();
    let group = i32::try_from(group).map_err(|_| ())?;
    loop {
        match probe.observe(identity) {
            Ok(ProbeObservation::ExactLive) => {
                // Native termination rechecks the exact birth/group binding before signaling.
                // A transient observation or signal failure retains this cleanup obligation.
                let _ = probe.terminate(identity);
            }
            Ok(ProbeObservation::ExactAbsent) => {
                if group_absent(group) {
                    return Ok(());
                }
                // Root absence does not prove the identity of a surviving numeric group. Keep
                // observing the unresolved tree rather than signaling a potentially reused ID.
            }
            Ok(ProbeObservation::Mismatched) => {
                if group_absent(group) {
                    return Ok(());
                }
                // The root PID was reused while the numeric group remains live. That does not
                // identify the surviving group as either the old tree or the reused process, so
                // retain the obligation without signaling either identity.
            }
            Ok(ProbeObservation::Unverifiable) | Err(_) => {}
        }
        std::thread::sleep(REOBSERVE_INTERVAL);
    }
}

#[allow(unsafe_code, reason = "signal zero only observes Linux process-group existence")]
fn group_absent(group: i32) -> bool {
    // SAFETY: `group` came from a nonzero root identity, was checked to equal the owned process
    // group, and converted to a positive i32. Signal zero performs no process mutation; absence
    // requires ESRCH from this exact observation rather than an elapsed patience window.
    if unsafe { libc::kill(-group, 0) } == 0 {
        return false;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}
