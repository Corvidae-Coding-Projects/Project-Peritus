//! Detect temporary principal ACEs left on newly created objects without editing caller ACLs.

use super::{AclObject, descriptor::Dacl, error};
use crate::WindowsError;
use core::ptr;
use std::collections::BTreeMap;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        ACL,
        Authorization::{
            ConvertStringSidToSidW, DENY_ACCESS, EXPLICIT_ACCESS_W, GetExplicitEntriesFromAclW,
            TRUSTEE_IS_SID,
        },
        CreatePrivateObjectSecurityEx, DestroyPrivateObjectSecurity, EqualSid, GENERIC_MAPPING,
        GROUP_SECURITY_INFORMATION, GetSecurityDescriptorDacl, OWNER_SECURITY_INFORMATION,
        SEF_AVOID_OWNER_CHECK, SEF_AVOID_PRIVILEGE_CHECK, SEF_DACL_AUTO_INHERIT,
    },
    Storage::FileSystem::{
        FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    },
};

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct Access {
    mask: u32,
    mode: i32,
    inheritance: u32,
}

struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns a LocalAlloc allocation returned by Win32.
        unsafe { LocalFree(self.0) };
    }
}

struct Inherited(*mut core::ffi::c_void);
impl Drop for Inherited {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the descriptor returned by the matching creator.
        unsafe { DestroyPrivateObjectSecurity(&raw const self.0) };
    }
}

pub(super) fn verify_new_child(
    child: &AclObject,
    parent: &AclObject,
    principal: &str,
    temporary: &[(bool, u32)],
) -> Result<(), WindowsError> {
    let principal = principal.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut sid = ptr::null_mut();
    // SAFETY: terminated SID text and a valid output pointer remain live for conversion.
    if unsafe { ConvertStringSidToSidW(principal.as_ptr(), &raw mut sid) } == 0 {
        return Err(error("transaction principal SID cannot be verified"));
    }
    let sid = Local(sid);
    let parent_descriptor = Dacl::read(&parent.file)?;
    // Use the real child's owner/group solely for creator-SID substitution. We intentionally
    // provide no creator DACL and no host token: this models only parent-derived inheritance,
    // never guesses a restricted/AppContainer creator's default/custom DACL.
    let creator = Dacl::read_components(
        &child.file,
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION,
    )?;
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut inherited = ptr::null_mut();
    // SAFETY: both complete descriptors, output storage, and filesystem mapping remain live.
    // The two AVOID flags permit a null token; no actual object's security is modified here.
    if unsafe {
        CreatePrivateObjectSecurityEx(
            parent_descriptor.as_ptr(),
            creator.as_ptr(),
            &raw mut inherited,
            ptr::null(),
            i32::from(child.directory),
            SEF_DACL_AUTO_INHERIT | SEF_AVOID_OWNER_CHECK | SEF_AVOID_PRIVILEGE_CHECK,
            ptr::null_mut(),
            &raw const mapping,
        )
    } == 0
    {
        return Err(error("original parent-derived inheritance cannot be computed"));
    }
    let inherited = Inherited(inherited);
    let mut expected = BTreeMap::<Access, usize>::new();
    for access in entries(inherited.0, sid.0)? {
        *expected.entry(access).or_default() += 1;
    }
    for access in entries(child.original.as_ptr(), sid.0)? {
        if let Some(count) = expected.get_mut(&access)
            && *count > 0
        {
            *count -= 1;
            continue;
        }
        if temporary
            .iter()
            .any(|(deny, mask)| (*deny == (access.mode == DENY_ACCESS)) && access.mask & mask != 0)
        {
            return Err(error(
                "new child retains a temporary principal ACE beyond restored inheritance",
            ));
        }
    }
    Ok(())
}

fn entries(
    descriptor: *mut core::ffi::c_void,
    sid: *mut core::ffi::c_void,
) -> Result<Vec<Access>, WindowsError> {
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl: *mut ACL = ptr::null_mut();
    // SAFETY: caller keeps the complete native descriptor alive throughout this inspection.
    if unsafe {
        GetSecurityDescriptorDacl(descriptor, &raw mut present, &raw mut acl, &raw mut defaulted)
    } == 0
    {
        return Err(error("new-child DACL cannot be decoded"));
    }
    if present == 0 || acl.is_null() {
        return Ok(Vec::new());
    }
    let mut count = 0;
    let mut entries: *mut EXPLICIT_ACCESS_W = ptr::null_mut();
    // SAFETY: ACL is borrowed from a valid descriptor; Win32 initializes the bounded output list.
    if unsafe { GetExplicitEntriesFromAclW(acl, &raw mut count, &raw mut entries) } != 0 {
        return Err(error("new-child ACE inventory cannot be decoded"));
    }
    let owned = Local(entries.cast());
    if count == 0 {
        return Ok(Vec::new());
    }
    if entries.is_null() || count > 65_535 {
        return Err(error("new-child ACE inventory is invalid"));
    }
    // SAFETY: the successful API returned count initialized records in the owned allocation.
    let entries = unsafe { core::slice::from_raw_parts(entries, count as usize) };
    let mut result = Vec::new();
    for entry in entries {
        if entry.Trustee.TrusteeForm != TRUSTEE_IS_SID || entry.Trustee.ptstrName.is_null() {
            return Err(error("new-child ACE trustee cannot be compared exactly"));
        }
        // SAFETY: TRUSTEE_IS_SID from the native decoder supplies a valid SID; conversion owns sid.
        if unsafe { EqualSid(entry.Trustee.ptstrName.cast(), sid) } != 0 {
            result.push(Access {
                mask: entry.grfAccessPermissions,
                mode: entry.grfAccessMode,
                // Protection may convert inherited ACEs to explicit while preserving authority.
                inheritance: entry.grfInheritance & !0x10,
            });
        }
    }
    drop(owned);
    Ok(result)
}
