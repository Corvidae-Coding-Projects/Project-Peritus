//! Hidden independent owner-death mode for exact daemon-launched process sessions.

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
        ProbeObservation::ExactAbsent => Ok(()),
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => Err(()),
    }
}
