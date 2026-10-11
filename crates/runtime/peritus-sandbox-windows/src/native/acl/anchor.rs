//! Atomically create and retain an owned deny directory, with no open-after-create race.

use super::{Stabilizers, error};
use crate::WindowsError;
use core::ptr;
use std::{
    fs::{File, OpenOptions},
    os::windows::{
        ffi::OsStrExt as _,
        fs::OpenOptionsExt as _,
        io::{AsRawHandle as _, FromRawHandle as _},
    },
    path::Path,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN_FOR_BACKUP_INTENT,
            FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
        },
    },
    Win32::{
        Foundation::{OBJ_CASE_INSENSITIVE, UNICODE_STRING},
        Storage::FileSystem::{
            DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

pub(crate) fn create(path: &Path) -> Result<File, WindowsError> {
    let parent = path.parent().ok_or_else(|| error("owned deny anchor has no parent"))?;
    let mut stable = Stabilizers::new();
    stable.acquire(parent)?;
    let directory = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(parent)
        .map_err(|_| error("deny anchor parent cannot be retained"))?;
    let name = path.file_name().ok_or_else(|| error("owned deny anchor has no final name"))?;
    let mut units = name.encode_wide().collect::<Vec<_>>();
    let length = u16::try_from(
        units.len().checked_mul(2).ok_or_else(|| error("deny anchor name length overflow"))?,
    )
    .map_err(|_| error("deny anchor name is too long"))?;
    let name = UNICODE_STRING { Length: length, MaximumLength: length, Buffer: units.as_mut_ptr() };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).expect("fixed Windows structure"),
        RootDirectory: directory.as_raw_handle(),
        ObjectName: &raw const name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut status = IO_STATUS_BLOCK::default();
    let mut handle = ptr::null_mut();
    // SAFETY: the stabilized parent handle and counted final component remain live, all output
    // storage is initialized, and FILE_CREATE cannot open/replace an existing object. Unlike
    // CreateDirectory+open, the returned handle owns the exact object created by this call.
    // https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile
    let result = unsafe {
        NtCreateFile(
            &raw mut handle,
            DELETE | READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &raw const attributes,
            &raw mut status,
            ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_FOR_BACKUP_INTENT,
            ptr::null(),
            0,
        )
    };
    if result < 0 {
        return Err(error("owned deny directory could not be created atomically"));
    }
    // SAFETY: successful NtCreateFile transferred the new directory handle to this owner.
    Ok(unsafe { File::from_raw_handle(handle) })
}
