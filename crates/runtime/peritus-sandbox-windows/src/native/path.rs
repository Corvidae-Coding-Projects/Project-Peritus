//! Windows volume identity query kept inside the inventoried FFI boundary.

use core::{mem::MaybeUninit, ptr};
use std::path::Path;

use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, GetVolumeInformationW, OPEN_EXISTING,
    },
};

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

/// Stable same-volume native identity used to reject path replacement during ACL recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativePathIdentity {
    volume_serial: u64,
    file_id: u64,
}

impl NativePathIdentity {
    pub(crate) fn new(volume_serial: u64, file_id: u64) -> Result<Self, WindowsError> {
        if volume_serial == 0 || file_id == 0 {
            return Err(path_error("native path identity is unavailable"));
        }
        Ok(Self { volume_serial, file_id })
    }

    pub(crate) const fn volume_serial(self) -> u64 { self.volume_serial }

    pub(crate) const fn file_id(self) -> u64 { self.file_id }
}

pub(crate) fn identity(path: &Path) -> Result<NativePathIdentity, WindowsError> {
    use std::os::windows::ffi::OsStrExt as _;

    let mut units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if units.is_empty() || units.len() >= 32_767 || units.contains(&0) {
        return Err(path_error("native path identity input is invalid"));
    }
    units.push(0);
    // SAFETY: `units` is a live NUL-terminated UTF-16 path, sharing is explicit, and no security
    // attributes or template handle are supplied.
    let handle = unsafe {
        CreateFileW(
            units.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(path_error("native path identity cannot be opened"));
    }
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: `handle` is open for attribute queries and the output points to writable storage.
    let observed = unsafe { GetFileInformationByHandle(handle, information.as_mut_ptr()) };
    // SAFETY: the non-null handle returned by CreateFileW is closed exactly once here.
    let _ = unsafe { CloseHandle(handle) };
    if observed == 0 {
        return Err(path_error("native path identity cannot be inspected"));
    }
    // SAFETY: GetFileInformationByHandle reported successful full initialization.
    let information = unsafe { information.assume_init() };
    NativePathIdentity::new(
        u64::from(information.dwVolumeSerialNumber),
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    )
}

pub(crate) fn volume_serial(path: &Path) -> Result<u64, WindowsError> {
    use std::os::windows::ffi::OsStrExt;

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if units.len() < 3
        || units.len() >= 32_767
        || units.contains(&0)
        || !matches!(units[0], 0x41..=0x5a | 0x61..=0x7a)
        || units[1] != u16::from(b':')
        || !matches!(units[2], 0x2f | 0x5c)
    {
        return Err(path_error("volume root is invalid"));
    }
    let root = [units[0], units[1], u16::from(b'\\'), 0];
    let mut serial = 0_u32;
    // SAFETY: the root is NUL terminated and all omitted output buffers use valid null/zero pairs.
    if unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            &raw mut serial,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        )
    } == 0
        || serial == 0
    {
        return Err(path_error("volume identity cannot be inspected"));
    }
    Ok(u64::from(serial))
}

fn path_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Path,
        WindowsOperation::ResolvePath,
        WindowsRecovery::CorrectRequest,
        detail,
    )
}
