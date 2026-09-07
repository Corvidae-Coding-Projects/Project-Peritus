//! Complete bounded libproc process-group counts for required resource samples.

use crate::MacosError;

use super::sample_error;

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "bounded libproc process ID query with no ownership transfer")]
pub(super) fn enumerate(group: i32, pids: &mut [i32]) -> Result<usize, MacosError> {
    use std::{ffi::c_void, mem::size_of_val};

    let buffer_bytes = i32::try_from(size_of_val(pids)).map_err(|_| sample_error())?;
    observe_group(pids.len(), || {
        // SAFETY: `pids` is writable for its checked byte length. This read-only query selects
        // exactly the owned process group, initializes integer PIDs and transfers no ownership.
        unsafe { libc::proc_listpgrppids(group, pids.as_mut_ptr().cast::<c_void>(), buffer_bytes) }
    })
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "thread-local errno is reset and observed on the calling thread around libproc"
)]
fn observe_group(capacity: usize, query: impl FnOnce() -> i32) -> Result<usize, MacosError> {
    // SAFETY: macOS provides a live, writable errno slot for this thread. No reference is made,
    // and no pointer is retained across the query. Clearing it distinguishes an empty group
    // from libproc's zero-on-error result even when an earlier call left errno nonzero.
    unsafe { *libc::__error() = 0 };
    let count = query();
    // SAFETY: macOS returns this thread's live errno slot; the value is copied without creating
    // a reference or retaining the pointer across threads.
    let error = unsafe { *libc::__error() };
    observed_count(count, error, capacity)
}

fn observed_count(count: i32, error: i32, capacity: usize) -> Result<usize, MacosError> {
    if count < 0 || (count == 0 && error != 0) {
        return Err(sample_error());
    }
    let count = usize::try_from(count).map_err(|_| sample_error())?;
    if count >= capacity {
        return Err(sample_error());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use crate::{MacosErrorKind, RecoveryAction};

    use super::*;

    #[test]
    fn libproc_zero_with_error_cannot_be_a_complete_resource_sample() {
        let error = observed_count(0, 5, 4).expect_err("query failure is not zero usage");
        assert_eq!(error.kind(), MacosErrorKind::SupervisorFailure);
        assert_eq!(error.recovery(), RecoveryAction::CancelAndReap);
    }

    #[test]
    fn libproc_success_and_capacity_bound_are_distinct_from_query_errors() {
        for count in 0..4 {
            assert_eq!(
                observed_count(count, 0, 4).expect("bounded"),
                usize::try_from(count).expect("positive")
            );
        }
        for count in [-1, 4, 5] {
            assert!(observed_count(count, 0, 4).is_err(), "invalid count {count}");
        }
        assert_eq!(
            observed_count(1, 5, 4).expect("positive successful results do not use errno"),
            1
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(unsafe_code, reason = "native test seeds and inspects only its own thread-local errno")]
    fn libproc_native_query_clears_stale_errno_before_an_empty_observation() {
        // SAFETY: the pointer names this thread's live writable errno slot; no reference escapes.
        unsafe { *libc::__error() = libc::EBUSY };
        assert_eq!(
            observe_group(4, || {
                // SAFETY: this is an immediate copy of this thread's errno, with no retained pointer.
                assert_eq!(unsafe { *libc::__error() }, 0, "clear errno before the query");
                0
            })
            .expect("empty native observation"),
            0
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(
        unsafe_code,
        reason = "native negative test exercises libproc with a valid but deliberately undersized buffer"
    )]
    fn libproc_native_failed_query_cannot_be_a_zero_resource_sample() {
        let mut pid = 0_i32;
        let mut raw_observation = (-1, 0);
        let error = observe_group(1, || {
            // SAFETY: one byte is within the live integer's allocation. The deliberately
            // undersized read-only query cannot write outside it or transfer ownership.
            let count =
                unsafe { libc::proc_listpgrppids(1, (&raw mut pid).cast::<std::ffi::c_void>(), 1) };
            // SAFETY: the errno slot is valid for this thread and read immediately after libproc.
            raw_observation = (count, unsafe { *libc::__error() });
            count
        })
        .expect_err("an unavailable query is not a complete resource observation");
        assert_eq!(raw_observation.0, 0);
        assert_ne!(raw_observation.1, 0);
        assert_eq!(error.kind(), MacosErrorKind::SupervisorFailure);
        assert_eq!(error.recovery(), RecoveryAction::CancelAndReap);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(
        unsafe_code,
        reason = "native positive test reads its own process group without transferring ownership"
    )]
    fn libproc_native_group_includes_the_current_process() {
        // SAFETY: getpgrp has no pointer arguments and only reads this process's group identity.
        let group = unsafe { libc::getpgrp() };
        let mut pids = vec![0_i32; 16_384];
        let count = enumerate(group, &mut pids).expect("native process group");
        assert!(pids[..count].contains(&i32::try_from(std::process::id()).expect("native PID")));
    }
}
