//! Read-only native diagnostic inventory for the unresolved protected-child regression.
#![allow(unsafe_code, reason = "read-only Win32 diagnosis of real fixture descriptor semantics")]

use core::ptr;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _},
    path::Path,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, LocalFree},
    Security::{
        ACL,
        Authorization::{
            ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GetExplicitEntriesFromAclW, TRUSTEE_IS_SID,
        },
        CreatePrivateObjectSecurityEx, DACL_SECURITY_INFORMATION, DestroyPrivateObjectSecurity,
        EqualSid, GENERIC_MAPPING, GROUP_SECURITY_INFORMATION, GetKernelObjectSecurity,
        GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorGroup,
        GetSecurityDescriptorOwner, OWNER_SECURITY_INFORMATION, SEF_AVOID_OWNER_CHECK,
        SEF_AVOID_PRIVILEGE_CHECK, SEF_DACL_AUTO_INHERIT,
    },
    Storage::FileSystem::{
        FILE_ALL_ACCESS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
    },
};

struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: the matching Win32 function transferred this LocalAlloc allocation.
        unsafe { LocalFree(self.0) };
    }
}

struct Inherited(*mut core::ffi::c_void);
impl Drop for Inherited {
    fn drop(&mut self) {
        // SAFETY: this descriptor was allocated by CreatePrivateObjectSecurityEx.
        unsafe { DestroyPrivateObjectSecurity(&raw const self.0) };
    }
}

pub(super) fn inspect(parent: &Path, child: &Path, directory: bool, principal: &str) -> String {
    let parent = read(parent, DACL_SECURITY_INFORMATION);
    let actual = read(child, DACL_SECURITY_INFORMATION);
    let creator = read(child, OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION);
    let text = principal.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut sid = ptr::null_mut();
    // SAFETY: terminated SID text and valid output pointer remain live for conversion.
    assert_ne!(unsafe { ConvertStringSidToSidW(text.as_ptr(), &raw mut sid) }, 0);
    let sid = Local(sid);
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut expected = ptr::null_mut();
    // SAFETY: complete aligned inputs and initialized output/mapping live through this call.
    // This computes a private descriptor only; it changes no filesystem object.
    let result = unsafe {
        CreatePrivateObjectSecurityEx(
            parent.as_ptr().cast_mut().cast(),
            creator.as_ptr().cast_mut().cast(),
            &raw mut expected,
            ptr::null(),
            i32::from(directory),
            SEF_DACL_AUTO_INHERIT | SEF_AVOID_OWNER_CHECK | SEF_AVOID_PRIVILEGE_CHECK,
            ptr::null_mut(),
            &raw const mapping,
        )
    };
    // SAFETY: obtains this thread's immediately preceding Win32 status for diagnostic failure.
    assert_ne!(result, 0, "private inheritance computation failed: {}", unsafe { GetLastError() });
    let expected = Inherited(expected);
    format!(
        "state=(control,dacl_present,dacl_null,owner_present,group_present); parent={:x?}; creator={:x?}; expected={:x?}; actual={:x?}; ACE=(mask,mode,raw_flags,flags_without_ID,count); expected_entries={:x?}; actual_entries={:x?}",
        state(parent.as_ptr().cast_mut().cast()),
        state(creator.as_ptr().cast_mut().cast()),
        state(expected.0),
        state(actual.as_ptr().cast_mut().cast()),
        entries(expected.0, sid.0),
        entries(actual.as_ptr().cast_mut().cast(), sid.0)
    )
}

fn read(path: &Path, information: u32) -> Vec<u32> {
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap();
    let mut length = 0;
    // SAFETY: live handle, zero capacity, and valid required-size output.
    unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            information,
            ptr::null_mut(),
            0,
            &raw mut length,
        )
    };
    assert!((20..=1_048_576).contains(&length));
    let mut words = vec![0_u32; (length as usize).div_ceil(4)];
    // SAFETY: aligned output has at least length bytes and the handle remains live.
    assert_ne!(
        unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                information,
                words.as_mut_ptr().cast(),
                length,
                &raw mut length,
            )
        },
        0
    );
    words
}

fn state(descriptor: *mut core::ffi::c_void) -> (u16, bool, bool, bool, bool) {
    let mut control = 0;
    let mut revision = 0;
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl: *mut ACL = ptr::null_mut();
    let mut owner = ptr::null_mut();
    let mut group = ptr::null_mut();
    // SAFETY: each caller retains its complete aligned descriptor for these read-only queries.
    unsafe {
        assert_ne!(
            GetSecurityDescriptorControl(descriptor, &raw mut control, &raw mut revision),
            0
        );
        assert_ne!(
            GetSecurityDescriptorDacl(
                descriptor,
                &raw mut present,
                &raw mut acl,
                &raw mut defaulted
            ),
            0
        );
        assert_ne!(GetSecurityDescriptorOwner(descriptor, &raw mut owner, &raw mut defaulted), 0);
        assert_ne!(GetSecurityDescriptorGroup(descriptor, &raw mut group, &raw mut defaulted), 0);
    }
    (control, present != 0, acl.is_null(), !owner.is_null(), !group.is_null())
}

fn entries(
    descriptor: *mut core::ffi::c_void,
    sid: *mut core::ffi::c_void,
) -> Vec<(u32, i32, u32, u32, usize)> {
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl: *mut ACL = ptr::null_mut();
    // SAFETY: caller retains a complete aligned descriptor and valid outputs remain live.
    assert_ne!(
        unsafe {
            GetSecurityDescriptorDacl(
                descriptor,
                &raw mut present,
                &raw mut acl,
                &raw mut defaulted,
            )
        },
        0
    );
    if present == 0 || acl.is_null() {
        return Vec::new();
    }
    let mut count = 0;
    let mut entries: *mut EXPLICIT_ACCESS_W = ptr::null_mut();
    // SAFETY: borrowed ACL and initialized output storage are valid.
    assert_eq!(unsafe { GetExplicitEntriesFromAclW(acl, &raw mut count, &raw mut entries) }, 0);
    let owned = Local(entries.cast());
    if count == 0 {
        return Vec::new();
    }
    assert!(!entries.is_null() && count <= 65_535);
    let mut counts = BTreeMap::new();
    // SAFETY: the successful decoder returned count initialized entries in the owned allocation.
    for entry in unsafe { core::slice::from_raw_parts(entries, count as usize) } {
        assert_eq!(entry.Trustee.TrusteeForm, TRUSTEE_IS_SID);
        assert!(!entry.Trustee.ptstrName.is_null());
        // SAFETY: both native-decoded and converted inputs are live valid SIDs.
        if unsafe { EqualSid(entry.Trustee.ptstrName.cast(), sid) } != 0 {
            let key = (entry.grfAccessPermissions, entry.grfAccessMode, entry.grfInheritance);
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    drop(owned);
    counts
        .into_iter()
        .map(|((mask, mode, flags), count)| (mask, mode, flags, flags & !0x10, count))
        .collect()
}
