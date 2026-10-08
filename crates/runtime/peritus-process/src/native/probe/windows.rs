//! Windows process creation-time recovery probe.

#![allow(
    unsafe_code,
    reason = "OpenProcess and GetProcessTimes are the narrow Windows birth-identity boundary"
)]

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE,
};
use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess};

use crate::{ProbeObservation, ProcessError, ProcessTreeIdentity, ProcessTreeQuiescence};

use super::{indeterminate, indeterminate_cause};

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

pub(super) fn observe(identity: ProcessTreeIdentity) -> Result<ProbeObservation, ProcessError> {
    let Some(expected_start) = exact_binding(identity) else {
        return Ok(ProbeObservation::Unverifiable);
    };
    match snapshot(identity.root_pid())? {
        Snapshot::Absent => Ok(ProbeObservation::ExactAbsent),
        Snapshot::Present { start_token, exited } => {
            if start_token != expected_start {
                Ok(ProbeObservation::Mismatched)
            } else if exited {
                Ok(ProbeObservation::ExactAbsent)
            } else {
                Ok(ProbeObservation::ExactLive)
            }
        }
    }
}

pub(super) fn terminate(identity: ProcessTreeIdentity) -> Result<(), ProcessError> {
    match observe(identity)? {
        ProbeObservation::ExactAbsent => Ok(()),
        ProbeObservation::Mismatched => {
            Err(indeterminate("Windows process identity changed before exact tree termination"))
        }
        ProbeObservation::Unverifiable => Err(indeterminate(
            "Windows process identity is unverifiable before exact tree termination",
        )),
        ProbeObservation::ExactLive => Err(indeterminate(
            "Windows recovery has no durable job handle for exact tree termination",
        )),
    }
}

pub(super) const fn observe_quiescence(
    _identity: ProcessTreeIdentity,
) -> Result<ProcessTreeQuiescence, ProcessError> {
    // A missing root query handle cannot prove that its job has no surviving descendants.
    // Recovery needs a durable reopenable job identity before it can assert complete absence.
    Ok(ProcessTreeQuiescence::Unverifiable)
}

const fn exact_binding(identity: ProcessTreeIdentity) -> Option<u64> {
    if identity.root_pid() != 0
        && identity.process_group().is_none()
        && identity.complete_containment()
    {
        identity.start_token()
    } else {
        None
    }
}

fn snapshot(pid: u32) -> Result<Snapshot, ProcessError> {
    let handle = match ProcessHandle::open(pid)? {
        OpenResult::Absent => return Ok(Snapshot::Absent),
        OpenResult::Handle(handle) => handle,
    };
    let mut creation = zero_file_time();
    let mut exit = zero_file_time();
    let mut kernel = zero_file_time();
    let mut user = zero_file_time();
    // SAFETY: `handle` owns a valid query handle and each output points to initialized writable
    // FILETIME storage retained until this call completes.
    if unsafe {
        GetProcessTimes(handle.0, &raw mut creation, &raw mut exit, &raw mut kernel, &raw mut user)
    } == 0
    {
        // SAFETY: GetLastError has no preconditions and is sampled immediately after the failed
        // GetProcessTimes call, before another platform operation can replace its diagnostic.
        let code = unsafe { GetLastError() };
        return Err(indeterminate_cause(
            "Windows process times cannot be observed",
            std::io::Error::from_raw_os_error(code as i32),
        ));
    }
    Ok(Snapshot::Present { start_token: file_time(creation), exited: file_time(exit) != 0 })
}

const fn zero_file_time() -> FILETIME {
    FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }
}

fn file_time(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(pid: u32) -> Result<OpenResult, ProcessError> {
        // SAFETY: no handle inheritance is requested and `pid` is a value-only process identity.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !handle.is_null() {
            return Ok(OpenResult::Handle(Self(handle)));
        }
        // SAFETY: GetLastError has no preconditions and is sampled immediately after OpenProcess.
        match unsafe { GetLastError() } {
            ERROR_INVALID_PARAMETER => Ok(OpenResult::Absent),
            code => Err(indeterminate_cause(
                "Windows process handle cannot be opened",
                std::io::Error::from_raw_os_error(code as i32),
            )),
        }
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper is created only from one successful OpenProcess call and owns the
        // non-null handle until this single drop.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

enum OpenResult {
    Absent,
    Handle(ProcessHandle),
}

enum Snapshot {
    Absent,
    Present { start_token: u64, exited: bool },
}
