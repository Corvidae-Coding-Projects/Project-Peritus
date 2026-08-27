//! Durable process-tree identity observations.

#![cfg_attr(
    any(target_os = "macos", target_os = "windows"),
    allow(
        unsafe_code,
        reason = "process birth-token observation is a narrow operating-system FFI boundary"
    )
)]

/// Exact root identity used to guard recovery against PID reuse.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessTreeIdentity {
    root_pid: u32,
    start_token: Option<u64>,
    process_group: Option<u32>,
    complete_containment: bool,
}

impl ProcessTreeIdentity {
    /// Creates one observed native process-tree identity.
    ///
    /// This is an observation value, not execution authority. C2 validates and persists the value
    /// before it contributes to lifecycle or recovery decisions.
    #[must_use]
    pub const fn new(
        root_pid: u32,
        start_token: Option<u64>,
        process_group: Option<u32>,
        complete_containment: bool,
    ) -> Self {
        Self { root_pid, start_token, process_group, complete_containment }
    }

    /// Returns the root operating-system process identifier.
    #[must_use]
    pub const fn root_pid(self) -> u32 {
        self.root_pid
    }
    /// Returns the backend-specific process birth token when observable.
    #[must_use]
    pub const fn start_token(self) -> Option<u64> {
        self.start_token
    }
    /// Returns the owned Unix process group/session leader when available.
    #[must_use]
    pub const fn process_group(self) -> Option<u32> {
        self.process_group
    }
    /// Returns whether complete descendant containment is available.
    #[must_use]
    pub const fn complete_containment(self) -> bool {
        self.complete_containment
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn current_start_token(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let fields: Vec<&str> = text.get(close + 2..)?.split_ascii_whitespace().collect();
    fields.get(19)?.parse().ok()
}

#[cfg(target_os = "macos")]
pub(crate) fn current_start_token(pid: u32) -> Option<u64> {
    use core::{
        ffi::c_void,
        mem::{MaybeUninit, size_of},
    };

    let pid = i32::try_from(pid).ok()?;
    let size = i32::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    let mut information = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    // SAFETY: the pointer names writable storage of exactly `size` bytes and is read only when
    // proc_pidinfo reports that it initialized the entire proc_bsdinfo value.
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
    // SAFETY: proc_pidinfo reported the exact complete output size above.
    let information = unsafe { information.assume_init() };
    if information.pbi_start_tvusec >= 1_000_000 {
        return None;
    }
    information
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(information.pbi_start_tvusec))
}

#[cfg(target_os = "windows")]
pub(crate) fn current_start_token(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> HANDLE;
        fn GetProcessTimes(
            process: HANDLE,
            creation: *mut FILETIME,
            exit: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> i32;
    }

    // SAFETY: handle inheritance is disabled and `pid` is passed by value.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: handle is open for query and each pointer names live writable FILETIME storage.
    let observed = unsafe {
        GetProcessTimes(handle, &raw mut creation, &raw mut exit, &raw mut kernel, &raw mut user)
    };
    // SAFETY: the non-null handle was returned by OpenProcess and is closed exactly once here.
    let _ = unsafe { CloseHandle(handle) };
    (observed != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}
