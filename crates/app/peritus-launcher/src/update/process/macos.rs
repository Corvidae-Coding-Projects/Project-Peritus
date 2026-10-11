//! Distinguish Darwin's zombie-only group signal error from a containment failure.

/// Called only after WNOWAIT observed the root's exit and while its owner keeps it unreaped.
/// A complete singleton group snapshot then proves there are no descendants left to signal.
#[allow(unsafe_code, reason = "libproc reads the membership of the still-owned process group")]
pub(super) fn only_group_member(root: i32) -> bool {
    // Apple proc_info.h: PROC_PGRP_ONLY. libc exposes proc_listpids but not this selector.
    const PROC_PGRP_ONLY: u32 = 2;
    let Ok(group) = u32::try_from(root) else { return false };
    // Two slots distinguish an exact singleton from a full (possibly truncated) listing.
    // proc_listpids includes both live processes and zombies under the process-list lock.
    let mut members = [0_i32; 2];
    let size = i32::try_from(size_of_val(&members)).expect("two PID slots fit i32");
    // SAFETY: the aligned, initialized array is writable for exactly size bytes. libproc
    // only borrows it for this call; the root PID remains pinned by the unreaped child.
    let written = unsafe {
        nix::libc::proc_listpids(PROC_PGRP_ONLY, group, members.as_mut_ptr().cast(), size)
    };
    // Errors, empty results, and additional members all retain the original signal error.
    written == i32::try_from(size_of::<i32>()).expect("PID size fits i32") && members[0] == root
}
