//! Windows volume identity query kept inside the inventoried FFI boundary.

use core::ptr;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

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

/// Compares exact native names with Windows ordinal ignore-case semantics.
pub(crate) fn compare_names(
    left: &std::ffi::OsStr,
    right: &std::ffi::OsStr,
) -> core::cmp::Ordering {
    use std::os::windows::ffi::OsStrExt as _;
    let left: Vec<u16> = left.encode_wide().collect();
    let right: Vec<u16> = right.encode_wide().collect();
    // Inputs are bounded by WindowsPath or the Win32 environment representation.
    let left_len = i32::try_from(left.len()).expect("bounded Windows name");
    let right_len = i32::try_from(right.len()).expect("bounded Windows name");
    // SAFETY: both buffers remain live for their explicit bounded lengths; no terminator is read.
    let result = unsafe {
        windows_sys::Win32::Globalization::CompareStringOrdinal(
            left.as_ptr(),
            left_len,
            right.as_ptr(),
            right_len,
            1,
        )
    };
    match result {
        1 => core::cmp::Ordering::Less,
        2 => core::cmp::Ordering::Equal,
        3 => core::cmp::Ordering::Greater,
        _ => left.cmp(&right),
    }
}
