//! DACL-only backup/restore; owner and SACL are never changed.

use super::error;
use crate::WindowsError;
use core::ptr;
use std::{fs::File, os::windows::io::AsRawHandle as _};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError},
    Security::{
        ACL,
        Authorization::{SE_FILE_OBJECT, SetSecurityInfo},
        DACL_SECURITY_INFORMATION, GetKernelObjectSecurity, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
        SetKernelObjectSecurity, UNPROTECTED_DACL_SECURITY_INFORMATION,
    },
};

#[derive(Eq, PartialEq)]
pub(super) struct Dacl {
    words: Vec<u32>,
    length: u32,
}

impl Dacl {
    pub(super) fn read(file: &File) -> Result<Self, WindowsError> {
        Self::read_components(file, DACL_SECURITY_INFORMATION)
    }

    pub(super) const fn as_ptr(&self) -> *mut core::ffi::c_void {
        self.words.as_ptr().cast_mut().cast()
    }

    pub(super) fn read_components(file: &File, information: u32) -> Result<Self, WindowsError> {
        let mut length = 0;
        // SAFETY: the handle is live; a null buffer with zero capacity only queries required size.
        let result = unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                information,
                ptr::null_mut(),
                0,
                &raw mut length,
            )
        };
        // SAFETY: this reads this thread's immediately preceding Win32 failure code.
        if result != 0
            || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
            || !(20..=1_048_576).contains(&length)
        {
            return Err(error("exact DACL backup size is unavailable or unbounded"));
        }
        let mut words = vec![0_u32; (length as usize).div_ceil(4)];
        let capacity = length;
        // SAFETY: u32 storage meets SECURITY_DESCRIPTOR alignment and has at least capacity bytes;
        // all initialized writable storage and the retained handle remain live for the call.
        if unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                information,
                words.as_mut_ptr().cast(),
                capacity,
                &raw mut length,
            )
        } == 0
            || length > capacity
        {
            return Err(error("exact DACL backup cannot be read"));
        }
        Ok(Self { words, length })
    }

    fn protection(&self) -> Result<u32, WindowsError> {
        let mut control = 0;
        let mut revision = 0;
        // SAFETY: words contains the complete aligned descriptor returned by Windows.
        if unsafe {
            GetSecurityDescriptorControl(
                self.words.as_ptr().cast_mut().cast(),
                &raw mut control,
                &raw mut revision,
            )
        } == 0
        {
            return Err(error("saved DACL control cannot be read"));
        }
        Ok(if control & SE_DACL_PROTECTED == 0 {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        } else {
            PROTECTED_DACL_SECURITY_INFORMATION
        })
    }

    pub(super) fn restore_inheritance(&self, file: &File) -> Result<(), WindowsError> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl: *mut ACL = ptr::null_mut();
        // SAFETY: the saved descriptor is complete, aligned, and remains live with its ACL.
        if unsafe {
            GetSecurityDescriptorDacl(
                self.words.as_ptr().cast_mut().cast(),
                &raw mut present,
                &raw mut acl,
                &raw mut defaulted,
            )
        } == 0
        {
            return Err(error("saved DACL cannot be decoded"));
        }
        // SAFETY: the retained exact object handle has WRITE_DAC; the optional ACL is borrowed
        // from the saved descriptor. Owner, group, and SACL are deliberately not requested.
        if unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | self.protection()?,
                ptr::null_mut(),
                ptr::null_mut(),
                acl,
                ptr::null_mut(),
            )
        } != 0
        {
            return Err(error("original inheritable DACL cannot be restored"));
        }
        Ok(())
    }

    pub(super) fn restore_exact(&self, file: &File) -> Result<(), WindowsError> {
        // SetSecurityInfo imposes current inheritance semantics on the subtree and can change a
        // legacy DACL, including null-versus-empty semantics. This second phase deliberately uses
        // the documented filesystem backup/restore exception for retained BACKUP_SEMANTICS handles:
        // https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights
        // https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-setkernelobjectsecurity
        // Unlike ordinary ACL editing, exact backup replay must not propagate again:
        // https://learn.microsoft.com/en-us/windows/win32/secauthz/automatic-propagation-of-inheritable-aces
        // SAFETY: the exact retained object handle has WRITE_DAC; the complete aligned original
        // descriptor remains live. Only DACL state is selected, never owner, group, or SACL.
        if unsafe {
            SetKernelObjectSecurity(
                file.as_raw_handle(),
                DACL_SECURITY_INFORMATION | self.protection()?,
                self.words.as_ptr().cast_mut().cast(),
            )
        } == 0
        {
            return Err(error("exact original DACL cannot be restored"));
        }
        if &Self::read(file)? != self {
            return Err(error("restored DACL bytes or control differ from the exact backup"));
        }
        Ok(())
    }
}
