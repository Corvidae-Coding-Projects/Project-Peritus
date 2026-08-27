//! macOS `proc_pidinfo` birth-token and process-session recovery probe.

#![allow(
    unsafe_code,
    reason = "proc_pidinfo is the narrow macOS process birth-identity observation boundary"
)]

use core::{
    ffi::c_void,
    mem::{MaybeUninit, size_of},
};

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
        Snapshot::Present { status, process_group, start_token } => {
            if start_token != expected_start || process_group != expected_group {
                Ok(ProbeObservation::Mismatched)
            } else if status == libc::SZOMB {
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
                "macOS process identity changed before exact tree termination",
            ));
        }
        ProbeObservation::Unverifiable => {
            return Err(indeterminate(
                "macOS process identity is unverifiable before exact tree termination",
            ));
        }
    }
    let group = identity
        .process_group()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| indeterminate("macOS process-group identity is not representable"))?;
    match killpg(Pid::from_raw(group), Signal::SIGKILL) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(_) => Err(indeterminate("macOS exact process-group termination failed")),
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
    let root_pid = pid;
    let pid = i32::try_from(root_pid)
        .map_err(|_| indeterminate("macOS process identity is not representable"))?;
    let size = i32::try_from(size_of::<libc::proc_bsdinfo>())
        .map_err(|_| indeterminate("macOS process observation size is not representable"))?;
    let mut information = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    // SAFETY: `__error` returns the calling thread's valid errno location on macOS.
    unsafe { *libc::__error() = 0 };
    // SAFETY: `information` points to writable storage of exactly `size` bytes for the requested
    // PROC_PIDTBSDINFO record; it is read only after proc_pidinfo reports that exact byte count.
    let observed = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            information.as_mut_ptr().cast::<c_void>(),
            size,
        )
    };
    if observed == 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => Ok(Snapshot::Absent),
            Some(libc::EPERM | libc::EACCES | 0) => Ok(Snapshot::Unverifiable),
            _ => Err(indeterminate("macOS process status cannot be observed")),
        };
    }
    if observed != size {
        return Ok(Snapshot::Unverifiable);
    }
    // SAFETY: the exact complete proc_bsdinfo byte count was reported above.
    let information = unsafe { information.assume_init() };
    if information.pbi_pid != root_pid || information.pbi_start_tvusec >= 1_000_000 {
        return Ok(Snapshot::Unverifiable);
    }
    let Some(start_token) = information
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(information.pbi_start_tvusec))
    else {
        return Ok(Snapshot::Unverifiable);
    };
    Ok(Snapshot::Present {
        status: information.pbi_status,
        process_group: information.pbi_pgid,
        start_token,
    })
}

enum Snapshot {
    Absent,
    Unverifiable,
    Present { status: u32, process_group: u32, start_token: u64 },
}
