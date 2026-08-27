//! Canonical durable live-instance record.

use crate::{DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery};

pub(super) struct InstanceRecord {
    bytes: Vec<u8>,
}

impl InstanceRecord {
    pub(super) fn current(identity: &DaemonIdentity) -> Result<Self, DaemonError> {
        let pid = std::process::id();
        let start_token = current_start_token(pid).ok_or_else(|| {
            DaemonError::new(
                DaemonErrorCode::Unsupported,
                DaemonRecovery::Operator,
                "observe daemon process identity",
                "operating system did not provide the daemon birth token",
            )
        })?;
        let text = format!(
            "version=1\nendpoint={}\npid={pid}\nstart_token={start_token}\n",
            identity.endpoint_name(),
        );
        Ok(Self { bytes: text.into_bytes() })
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(target_os = "linux")]
fn current_start_token(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    text.get(close + 2..)?.split_ascii_whitespace().nth(19)?.parse().ok()
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "proc_pidinfo is the narrow daemon birth-token TCB")]
fn current_start_token(pid: u32) -> Option<u64> {
    use core::{
        ffi::c_void,
        mem::{MaybeUninit, size_of},
    };
    let pid = i32::try_from(pid).ok()?;
    let size = i32::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    let mut information = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    // SAFETY: the pointer names exact writable proc_bsdinfo storage and is read only after an
    // exact-size success result.
    let observed = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            information.as_mut_ptr().cast::<c_void>(),
            size,
        )
    };
    if observed != size {
        return None;
    }
    // SAFETY: proc_pidinfo reported complete initialization above.
    let information = unsafe { information.assume_init() };
    information
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(information.pbi_start_tvusec))
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code, reason = "GetProcessTimes is the narrow daemon birth-token TCB")]
fn current_start_token(pid: u32) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // SAFETY: handle inheritance is disabled and pid is passed by value.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: the handle is query-capable and output pointers name live FILETIME storage.
    let result = unsafe {
        GetProcessTimes(handle, &raw mut creation, &raw mut exit, &raw mut kernel, &raw mut user)
    };
    // SAFETY: the non-null owned handle is closed exactly once.
    let _ = unsafe { CloseHandle(handle) };
    (result != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}
