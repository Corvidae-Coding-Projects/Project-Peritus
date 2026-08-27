//! Linux `/proc` birth-token and process-session recovery probe.

use std::{io::ErrorKind, path::Path};

use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};

use crate::{ProbeObservation, ProcessError, ProcessTreeIdentity};

use super::indeterminate;

pub(super) fn observe(identity: ProcessTreeIdentity) -> Result<ProbeObservation, ProcessError> {
    let Some((expected_start, expected_group)) = exact_binding(identity) else {
        return Ok(ProbeObservation::Unverifiable);
    };
    match snapshot(identity.root_pid())? {
        Snapshot::Absent => Ok(ProbeObservation::ExactAbsent),
        Snapshot::Unverifiable => Ok(ProbeObservation::Unverifiable),
        Snapshot::Present { state, process_group, start_token } => {
            if start_token != expected_start || process_group != expected_group {
                Ok(ProbeObservation::Mismatched)
            } else if matches!(state, b'Z' | b'X' | b'x') {
                Ok(ProbeObservation::Unverifiable)
            } else {
                Ok(ProbeObservation::ExactLive)
            }
        }
    }
}

pub(super) fn terminate(identity: ProcessTreeIdentity) -> Result<(), ProcessError> {
    match observe(identity)? {
        ProbeObservation::ExactLive => {}
        ProbeObservation::ExactAbsent => return Ok(()),
        ProbeObservation::Mismatched => {
            return Err(indeterminate(
                "Linux process identity changed before exact tree termination",
            ));
        }
        ProbeObservation::Unverifiable => {
            return Err(indeterminate(
                "Linux process identity is unverifiable before exact tree termination",
            ));
        }
    }
    let group = identity
        .process_group()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| indeterminate("Linux process-group identity is not representable"))?;
    match killpg(Pid::from_raw(group), Signal::SIGKILL) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(_) => Err(indeterminate("Linux exact process-group termination failed")),
    }
}

fn exact_binding(identity: ProcessTreeIdentity) -> Option<(u64, u32)> {
    let root = identity.root_pid();
    let group = identity.process_group()?;
    let start = identity.start_token()?;
    (root != 0 && i32::try_from(root).is_ok() && group == root && identity.complete_containment())
        .then_some((start, group))
}

fn snapshot(pid: u32) -> Result<Snapshot, ProcessError> {
    if !Path::new("/proc/self/stat").is_file() {
        return Err(indeterminate("Linux procfs process observations are unavailable"));
    }
    let text = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Snapshot::Absent),
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            return Ok(Snapshot::Unverifiable);
        }
        Err(_) => return Err(indeterminate("Linux process status cannot be read")),
    };
    Ok(parse_stat(pid, &text).unwrap_or(Snapshot::Unverifiable))
}

fn parse_stat(expected_pid: u32, text: &str) -> Option<Snapshot> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open || text.get(..open)?.trim().parse::<u32>().ok()? != expected_pid {
        return None;
    }
    let after_name = close.checked_add(1)?;
    let mut fields = text.get(after_name..)?.split_ascii_whitespace();
    let state = fields.next()?.as_bytes();
    if state.len() != 1 {
        return None;
    }
    let _parent_pid = fields.next()?;
    let process_group = fields.next()?.parse().ok()?;
    let start_token = fields.nth(16)?.parse().ok()?;
    Some(Snapshot::Present { state: state[0], process_group, start_token })
}

enum Snapshot {
    Absent,
    Unverifiable,
    Present { state: u8, process_group: u32, start_token: u64 },
}
