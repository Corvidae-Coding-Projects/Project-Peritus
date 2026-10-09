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
    let size = i32::try_from(std::mem::size_of_val(&members)).expect("two PID slots fit i32");
    // SAFETY: the aligned, initialized array is writable for exactly size bytes. libproc
    // only borrows it for this call; the root PID remains pinned by the unreaped child.
    let written = unsafe {
        nix::libc::proc_listpids(PROC_PGRP_ONLY, group, members.as_mut_ptr().cast(), size)
    };
    // Errors, empty results, and additional members all retain the original signal error.
    written == i32::try_from(std::mem::size_of::<i32>()).expect("PID size fits i32")
        && members[0] == root
}

#[cfg(test)]
mod tests {
    use super::super::{ChildOwner, root_exited, spawn};
    use super::only_group_member;
    use std::{process::Command, time::Duration};

    async fn exited_root(script: &str) -> (ChildOwner, i32) {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        let child = spawn(&mut command, "macOS root fixture").unwrap();
        let pid = i32::try_from(child.0.id().unwrap()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root_exited(pid).unwrap() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture root exits");
        (child, pid)
    }

    #[tokio::test]
    async fn reaps_a_completed_singleton_group_with_its_exact_exit_status() {
        let (mut child, pid) = exited_root("exit 17").await;
        assert!(only_group_member(pid), "unreaped root remains the sole group member");
        assert_eq!(child.wait().await.unwrap().code(), Some(17));
    }

    #[tokio::test]
    async fn live_descendant_prevents_singleton_exception() {
        let (mut child, pid) = exited_root("sleep 30 & exit 0").await;
        assert!(!only_group_member(pid), "live descendant requires group termination");
        assert!(child.wait().await.unwrap().success());
    }
}
