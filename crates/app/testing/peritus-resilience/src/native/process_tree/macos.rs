//! Bounded macOS controller process-group observation.

use nix::errno::Errno;

use crate::{SubjectError, SubjectErrorCode};

use super::super::subject_error;

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "inventoried libproc read-only process-group enumeration boundary")]
pub(super) fn process_group_is_empty(group: i32) -> Result<bool, SubjectError> {
    use std::{ffi::c_void, mem::size_of_val};

    const MAX_GROUP_PROCESSES: usize = 16_384;

    let mut pids = vec![0_i32; MAX_GROUP_PROCESSES];
    let buffer_bytes = i32::try_from(size_of_val(pids.as_slice())).map_err(|_| {
        subject_error(
            SubjectErrorCode::Cleanup,
            "macOS process-group buffer exceeds platform capacity",
            false,
        )
    })?;
    observe_group(pids.len(), || {
        // SAFETY: `pids` is writable for `buffer_bytes`; libproc reads the exact controller-owned
        // process group and transfers no ownership.
        unsafe { libc::proc_listpgrppids(group, pids.as_mut_ptr().cast::<c_void>(), buffer_bytes) }
    })
}

fn observe_group(capacity: usize, query: impl FnOnce() -> i32) -> Result<bool, SubjectError> {
    // libproc uses zero for both an empty group and a failed query. Discard an earlier call's
    // errno and capture this query's errno before any subsequent work can overwrite it.
    Errno::clear();
    let count = query();
    let error = Errno::last_raw();
    if count < 0 || (count == 0 && error != 0) {
        return Err(subject_error(
            SubjectErrorCode::Cleanup,
            format!(
                "enumerate owned macOS controller process group failed (count {count}, errno {error})"
            ),
            false,
        ));
    }
    let count = usize::try_from(count).map_err(|_| {
        subject_error(
            SubjectErrorCode::Cleanup,
            "macOS process-group count exceeds platform capacity",
            false,
        )
    })?;
    if count >= capacity {
        return Err(subject_error(
            SubjectErrorCode::Cleanup,
            "macOS process-group observation exceeded its bounded buffer",
            false,
        ));
    }
    Ok(count == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libproc_zero_with_error_cannot_prove_cleanup() {
        let error = observe_group(4, || {
            Errno::EIO.set();
            0
        })
        .expect_err("an unavailable observation is not an empty process group");
        assert_eq!(error.code(), SubjectErrorCode::Cleanup);
        assert!(error.context().as_str().contains("count 0, errno 5"));
    }

    #[test]
    fn libproc_query_clears_stale_errno_before_observing_empty_group() {
        Errno::EBUSY.set();
        assert!(
            observe_group(4, || {
                assert_eq!(Errno::last_raw(), 0, "clear errno before the system call");
                0
            })
            .expect("a successful empty observation")
        );
    }

    #[test]
    fn libproc_counts_remain_bounded_and_negative_results_fail() {
        assert!(observe_group(4, || 0).expect("empty"));
        assert!(!observe_group(4, || 1).expect("one process"));
        assert!(!observe_group(4, || 3).expect("bounded processes"));
        assert!(
            !observe_group(4, || {
                Errno::EIO.set();
                1
            })
            .expect("positive successful results do not use errno")
        );
        for count in [-1, 4, 5] {
            assert!(observe_group(4, || count).is_err(), "invalid count {count}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(
        unsafe_code,
        reason = "native negative test uses a valid buffer with an undersized libproc byte count"
    )]
    fn libproc_native_failed_query_returns_zero_but_cannot_prove_cleanup() {
        let mut pid = 0_i32;
        let mut raw_observation = (-1, 0);
        let error = observe_group(1, || {
            // SAFETY: the integer is live and writable; declaring one byte is smaller than its
            // actual allocation and deliberately exercises libproc's bounded-buffer error.
            let count =
                unsafe { libc::proc_listpgrppids(1, (&raw mut pid).cast::<std::ffi::c_void>(), 1) };
            raw_observation = (count, Errno::last_raw());
            count
        })
        .expect_err("native query failure must reject cleanup");
        assert_eq!(raw_observation.0, 0);
        assert_ne!(raw_observation.1, 0);
        assert_eq!(error.code(), SubjectErrorCode::Cleanup);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn libproc_native_live_group_is_not_empty() {
        assert!(!process_group_is_empty(nix::unistd::getpgrp().as_raw()).expect("native group"));
    }
}
