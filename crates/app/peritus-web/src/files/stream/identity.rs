//! Exact Windows file identity from owned handles, independent of pathname reuse.
#![allow(unsafe_code)]

use crate::error::Result;
use std::{fs::File, mem::MaybeUninit, os::windows::io::AsRawHandle as _};
use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};

pub(super) fn file(file:&File)->Result<(u32,u64)> {
    let mut information=MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: file owns the live handle for the call; information provides the
    // required writable aligned storage and is read only after API success.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(),information.as_mut_ptr()) }==0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: a successful GetFileInformationByHandle initialized every field.
    let information=unsafe { information.assume_init() };
    let index=(u64::from(information.nFileIndexHigh)<<32)|u64::from(information.nFileIndexLow);
    Ok((information.dwVolumeSerialNumber,index))
}
