//! Windows volume identity query kept inside the inventoried FFI boundary.

use core::ptr;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

pub(crate) fn volume_serial(path: &Path) -> Result<u64, WindowsError> {
    let text = path.to_string_lossy();
    let drive = text.get(..2).ok_or_else(|| path_error("volume root is invalid"))?;
    let root = format!("{drive}\\").encode_utf16().chain(core::iter::once(0)).collect::<Vec<_>>();
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
