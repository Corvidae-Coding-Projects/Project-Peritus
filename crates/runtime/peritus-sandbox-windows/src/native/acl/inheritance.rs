//! Detect temporary principal ACEs left on newly created objects without editing caller ACLs.

use super::{AclObject, descriptor::Dacl, error};
use crate::WindowsError;
use core::ptr;
use std::collections::BTreeMap;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{ConvertStringSidToSidW, DENY_ACCESS, GRANT_ACCESS},
        CreatePrivateObjectSecurityEx, DestroyPrivateObjectSecurity, EqualSid, GENERIC_MAPPING,
        GROUP_SECURITY_INFORMATION, GetAce, GetAclInformation, GetLengthSid,
        GetSecurityDescriptorDacl, INHERITED_ACE, IsValidAcl, IsValidSid,
        OWNER_SECURITY_INFORMATION, SEF_AVOID_OWNER_CHECK, SEF_AVOID_PRIVILEGE_CHECK,
        SEF_DACL_AUTO_INHERIT,
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
    // The descriptor owns a native ACL allocation; establish its declared bounds before
    // inspecting any ACE. GetExplicitEntriesFromAcl omits inherited ACEs and cannot be used
    // for either the original-parent baseline or the child's residual-authority inventory.
    // SAFETY: GetSecurityDescriptorDacl returned this ACL header in a live native descriptor.
    let allocated = usize::from(unsafe { (*acl).AclSize });
    if allocated < size_of::<ACL>() {
        return Err(error("new-child ACL header is truncated"));
    }
    let mut size = ACL_SIZE_INFORMATION::default();
    // SAFETY: the live native ACL and initialized output storage are valid for these queries.
    if unsafe { IsValidAcl(acl) } == 0
        || unsafe {
            GetAclInformation(
                acl,
                (&raw mut size).cast(),
                u32::try_from(size_of::<ACL_SIZE_INFORMATION>())
                    .map_err(|_| error("ACL inventory size cannot be represented"))?,
                AclSizeInformation,
            )
        } == 0
    {
        return Err(error("new-child ACL inventory is invalid"));
    }
    let used = usize::try_from(size.AclBytesInUse)
        .map_err(|_| error("ACL inventory length cannot be represented"))?;
    if used < size_of::<ACL>() || used > allocated || size.AceCount > 65_535 {
        return Err(error("new-child ACL inventory exceeds its allocation"));
    }
    let mut result = Vec::new();
    for index in 0..size.AceCount {
        let mut ace = ptr::null_mut();
        // SAFETY: validated ACL, bounded index, and initialized pointer output remain live.
        if unsafe { GetAce(acl, index, &raw mut ace) } == 0 || ace.is_null() {
            return Err(error("new-child ACE cannot be read"));
        }
        if let Some(access) = decode_ace(acl, used, ace, sid)? {
            result.push(access);
        }
    }
    Ok(result)
}

fn decode_ace(
    acl: *mut ACL,
    used: usize,
    ace: *mut core::ffi::c_void,
    sid: *mut core::ffi::c_void,
) -> Result<Option<Access>, WindowsError> {
    let offset = ace
        .addr()
        .checked_sub(acl.addr())
        .ok_or_else(|| error("new-child ACE precedes its ACL"))?;
    if offset < size_of::<ACL>() || !fits(offset, size_of::<ACE_HEADER>(), used) {
        return Err(error("new-child ACE header exceeds its ACL"));
    }
    // SAFETY: the complete header lies inside the validated ACL; unaligned read is supported.
    let header = unsafe { ptr::read_unaligned(ace.cast::<ACE_HEADER>()) };
    let length = usize::from(header.AceSize);
    if length < size_of::<ACCESS_ALLOWED_ACE>() || !fits(offset, length, used) {
        return Err(error("new-child ACE exceeds its ACL"));
    }
    // Standard allow (0) and deny (1) ACEs share this mask/SID-prefix layout. Object,
    // callback, and all other ACE layouts need separate parsers; never silently omit them.
    let mode = match header.AceType {
        0 => GRANT_ACCESS,
        1 => DENY_ACCESS,
        _ => return Err(error("new-child ACE type cannot be compared exactly")),
    };
    // SAFETY: the entire standard prefix has been bounded within this ACE/ACL allocation.
    let basic = unsafe { ptr::read_unaligned(ace.cast::<ACCESS_ALLOWED_ACE>()) };
    let local_base = (&raw const basic).addr();
    let sid_offset = (&raw const basic.SidStart)
        .addr()
        .checked_sub(local_base)
        .ok_or_else(|| error("standard ACE SID offset is invalid"))?;
    if !fits(sid_offset, 8, length) {
        return Err(error("new-child ACE SID header is truncated"));
    }
    // SAFETY: sid_offset plus the complete eight-byte SID header lies inside this ACE.
    let trustee = unsafe { ace.cast::<u8>().add(sid_offset) };
    // SAFETY: the SID header's subauthority count byte has been bounded above.
    let subauthorities = unsafe { *trustee.add(1) };
    let sid_length = 8_usize + 4 * usize::from(subauthorities);
    if !fits(sid_offset, sid_length, length) {
        return Err(error("new-child ACE SID exceeds its ACE"));
    }
    // SAFETY: all bytes implied by the SID header lie in this ACE, proven before native
    // validation or length computation; the principal SID was returned by native conversion.
    if unsafe { IsValidSid(trustee.cast()) } == 0
        || usize::try_from(unsafe { GetLengthSid(trustee.cast()) }).ok() != Some(sid_length)
    {
        return Err(error("new-child ACE SID is invalid"));
    }
    // SAFETY: both SIDs are live and validated, including the trustee's complete byte extent.
    if unsafe { EqualSid(trustee.cast(), sid) } == 0 {
        return Ok(None);
    }
    Ok(Some(Access {
        mask: basic.Mask,
        mode,
        // Protection may make an inherited ACE explicit without changing its authority.
        inheritance: u32::from(header.AceFlags) & !INHERITED_ACE,
    }))
}

fn fits(offset: usize, length: usize, total: usize) -> bool {
    offset.checked_add(length).is_some_and(|end| end <= total)
}
