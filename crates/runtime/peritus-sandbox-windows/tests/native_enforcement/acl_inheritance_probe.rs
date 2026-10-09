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
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{
            ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GetExplicitEntriesFromAclW, TRUSTEE_IS_SID,
        },
        CreatePrivateObjectSecurityEx, DACL_SECURITY_INFORMATION, DestroyPrivateObjectSecurity,
        EqualSid, GENERIC_MAPPING, GROUP_SECURITY_INFORMATION, GetAce, GetAclInformation,
        GetKernelObjectSecurity, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
        GetSecurityDescriptorGroup, GetSecurityDescriptorOwner, InitializeSecurityDescriptor,
        IsValidAcl, OWNER_SECURITY_INFORMATION, SECURITY_DESCRIPTOR, SEF_AVOID_OWNER_CHECK,
        SEF_AVOID_PRIVILEGE_CHECK, SEF_DACL_AUTO_INHERIT, SetSecurityDescriptorGroup,
        SetSecurityDescriptorOwner,
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

pub(super) fn inspect(
    parent_path: &Path,
    child: &Path,
    directory: bool,
    principal: &str,
) -> String {
    let parent = read(parent_path, DACL_SECURITY_INFORMATION);
    let actual = read(child, DACL_SECURITY_INFORMATION);
    let creator = read(child, OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION);
    let text = principal.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut sid = ptr::null_mut();
    // SAFETY: terminated SID text and valid output pointer remain live for conversion.
    assert_ne!(unsafe { ConvertStringSidToSidW(text.as_ptr(), &raw mut sid) }, 0);
    let sid = Local(sid);
    let full_parent = read(
        parent_path,
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
    );
    let creator_pointer = creator.as_ptr().cast_mut().cast();
    let mut owner = ptr::null_mut();
    let mut group = ptr::null_mut();
    let mut defaulted = 0;
    let mut fresh = SECURITY_DESCRIPTOR::default();
    let fresh_pointer = (&raw mut fresh).cast();
    // SAFETY: creator owns the queried SID bytes through every matrix computation; fresh is
    // writable absolute descriptor storage, initialized before its owner/group are assigned.
    unsafe {
        assert_ne!(
            GetSecurityDescriptorOwner(creator_pointer, &raw mut owner, &raw mut defaulted),
            0
        );
        assert_ne!(
            GetSecurityDescriptorGroup(creator_pointer, &raw mut group, &raw mut defaulted),
            0
        );
        assert_ne!(InitializeSecurityDescriptor(fresh_pointer, 1), 0);
        assert_ne!(SetSecurityDescriptorOwner(fresh_pointer, owner, 0), 0);
        assert_ne!(SetSecurityDescriptorGroup(fresh_pointer, group, 0), 0);
    }
    let mut matrix = Vec::new();
    for (parent_name, parent) in [("dacl_parent", &parent), ("full_parent", &full_parent)] {
        for (creator_name, creator) in
            [("captured_creator", creator_pointer), ("fresh_creator", fresh_pointer)]
        {
            matrix.push(format!(
                "{parent_name}/{creator_name} {}",
                compute(parent.as_ptr().cast_mut().cast(), creator, directory, sid.0)
            ));
        }
    }
    format!(
        "state=(control,dacl_present,dacl_null,owner_present,group_present); parent={:x?}; full_parent={:x?}; creator={:x?}; fresh_creator={:x?}; actual={:x?}; ACE=(mask,mode,raw_flags,flags_without_ID,count); parent_entries={:x?}; full_parent_entries={:x?}; actual_entries={:x?}; parent_raw={}; actual_raw={}; matrix={matrix:?}",
        state(parent.as_ptr().cast_mut().cast()),
        state(full_parent.as_ptr().cast_mut().cast()),
        state(creator_pointer),
        state(fresh_pointer),
        state(actual.as_ptr().cast_mut().cast()),
        entries(parent.as_ptr().cast_mut().cast(), sid.0),
        entries(full_parent.as_ptr().cast_mut().cast(), sid.0),
        entries(actual.as_ptr().cast_mut().cast(), sid.0),
        raw_entries(parent.as_ptr().cast_mut().cast(), sid.0),
        raw_entries(actual.as_ptr().cast_mut().cast(), sid.0)
    )
}

fn compute(
    parent: *mut core::ffi::c_void,
    creator: *mut core::ffi::c_void,
    directory: bool,
    sid: *mut core::ffi::c_void,
) -> String {
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut expected = ptr::null_mut();
    // SAFETY: caller retains complete descriptors and SID; output and mapping live through
    // the call. This computes a private descriptor only and changes no filesystem object.
    let result = unsafe {
        CreatePrivateObjectSecurityEx(
            parent,
            creator,
            &raw mut expected,
            ptr::null(),
            i32::from(directory),
            SEF_DACL_AUTO_INHERIT | SEF_AVOID_OWNER_CHECK | SEF_AVOID_PRIVILEGE_CHECK,
            ptr::null_mut(),
            &raw const mapping,
        )
    };
    if result == 0 {
        // SAFETY: reads this thread's immediately preceding API failure.
        return format!("error={}", unsafe { GetLastError() });
    }
    let expected = Inherited(expected);
    format!(
        "state={:x?}, entries={:x?}, raw={}",
        state(expected.0),
        entries(expected.0, sid),
        raw_entries(expected.0, sid)
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

/// Independent raw inventory includes inherited ACEs even if the high-level decoder omits them.
fn raw_entries(descriptor: *mut core::ffi::c_void, sid: *mut core::ffi::c_void) -> String {
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl: *mut ACL = ptr::null_mut();
    // SAFETY: callers retain complete native descriptors and valid output storage.
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
        return "none".to_owned();
    }
    let mut size = ACL_SIZE_INFORMATION::default();
    // SAFETY: this descriptor's native ACL and initialized output storage remain live.
    unsafe {
        assert_ne!(IsValidAcl(acl), 0);
        assert_ne!(
            GetAclInformation(
                acl,
                (&raw mut size).cast(),
                u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).unwrap(),
                AclSizeInformation
            ),
            0
        );
    }
    let count = size.AceCount;
    assert!(count <= 65_535);
    let mut matched = Vec::new();
    let mut unsupported = Vec::new();
    for index in 0..count {
        let mut ace = ptr::null_mut();
        // SAFETY: index is bounded by the native ACL's count and output storage is valid.
        assert_ne!(unsafe { GetAce(acl, index, &raw mut ace) }, 0);
        assert!(!ace.is_null());
        // SAFETY: GetAce returned this valid header from the native ACL.
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if header.AceType > 1 {
            unsupported.push((header.AceType, header.AceFlags, header.AceSize));
            continue;
        }
        assert!(usize::from(header.AceSize) >= size_of::<ACCESS_ALLOWED_ACE>());
        // SAFETY: native standard allow/deny ACEs have the same mask/SID-prefix layout.
        let basic = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid_offset = (&raw const basic.SidStart).addr().checked_sub(ace.addr()).unwrap();
        assert!(usize::from(header.AceSize) >= sid_offset + 8);
        let trustee = (&raw const basic.SidStart).cast_mut().cast();
        // SAFETY: the bounded standard SID header has at least eight bytes inside this ACE.
        let subauthorities = unsafe { *(&raw const basic.SidStart).cast::<u8>().add(1) };
        assert!(sid_offset + 8 + 4 * usize::from(subauthorities) <= usize::from(header.AceSize));
        // SAFETY: native ACL standard ACE and converted sid contain valid live SIDs.
        if unsafe { EqualSid(trustee, sid) } != 0 {
            matched.push((header.AceType, basic.Mask, header.AceFlags, header.AceSize));
        }
    }
    format!(
        "count={count}, matched(type,mask,flags,size)={matched:x?}, unsupported={unsupported:x?}"
    )
}
