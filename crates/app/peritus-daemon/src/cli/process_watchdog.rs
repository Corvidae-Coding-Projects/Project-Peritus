//! Hidden independent owner-death mode for exact daemon-launched process groups.

use std::{
    io::Read as _,
    process::ExitCode,
    time::{Duration, Instant},
};

use peritus_process::{NativeProcessProbe, ProbeObservation, ProcessProbe, ProcessTreeIdentity};

const DISARM: u8 = 1;
const REOBSERVE_LIMIT: Duration = Duration::from_millis(500);

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
    let mut observation = probe.observe(identity).map_err(|_| ())?;
    if observation == ProbeObservation::Unverifiable {
        let deadline = Instant::now() + REOBSERVE_LIMIT;
        while observation == ProbeObservation::Unverifiable && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            observation = probe.observe(identity).map_err(|_| ())?;
        }
    }
    match observation {
        ProbeObservation::ExactLive => probe.terminate(identity).map_err(|_| ()),
        ProbeObservation::ExactAbsent => {
            let group = i32::try_from(group).map_err(|_| ())?;
            kill_group(group)
        }
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => Err(()),
    }
}

#[allow(unsafe_code, reason = "negative-pid kill is the Linux process-group termination boundary")]
fn kill_group(group: i32) -> Result<(), ()> {
    // SAFETY: `group` came from a nonzero root identity, was checked to equal the owned process
    // group, and converted to a positive i32. Negating it selects that exact process group.
    if unsafe { libc::kill(-group, libc::SIGKILL) } == 0 {
        return Ok(());
    }
    (std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)).then_some(()).ok_or(())
}
