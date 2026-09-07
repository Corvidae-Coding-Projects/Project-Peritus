//! Optional macOS process counts; root handles retain exit authority.

use nix::errno::Errno;

use crate::ProcessError;

use super::sample_error;

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "inventoried libproc read-only process-group enumeration boundary")]
pub(crate) fn process_group_count(
    identity: crate::ProcessTreeIdentity,
) -> Result<Option<u64>, ProcessError> {
    use std::{ffi::c_void, mem::size_of_val};

    const MAX_GROUP_PROCESSES: usize = 16_384;

    let group = identity
        .process_group()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| sample_error("process-group identity is unavailable"))?;
    let mut pids = vec![0_i32; MAX_GROUP_PROCESSES];
    let buffer_bytes = i32::try_from(size_of_val(pids.as_slice()))
        .map_err(|_| sample_error("process-group buffer exceeds platform capacity"))?;
    let count = observe_group(pids.len(), || {
        // SAFETY: `pids` is writable for `buffer_bytes`; the selector requests process IDs for the
        // exact C2-owned process group and transfers no ownership.
        unsafe { libc::proc_listpgrppids(group, pids.as_mut_ptr().cast::<c_void>(), buffer_bytes) }
    })?;
    let Some(count) = count else { return Ok(None) };
    pids.truncate(count);
    pids.retain(|pid| *pid > 0);
    pids.sort_unstable();
    pids.dedup();
    Ok(Some(u64::try_from(pids.len()).unwrap_or(u64::MAX)))
}

fn observe_group(
    capacity: usize,
    query: impl FnOnce() -> i32,
) -> Result<Option<usize>, ProcessError> {
    // A zero libproc result can mean either an empty group or an unavailable query. Only errno
    // from this exact call can distinguish them; successful calls need not clear stale errno.
    Errno::clear();
    let count = query();
    let error = Errno::last_raw();
    if count < 0 || (count == 0 && error != 0) {
        // The first optional sample may be unavailable for a short-lived child. The root handle
        // and separate process-group quiescence check remain the exit and cleanup authorities.
        return Ok(None);
    }
    let count = usize::try_from(count)
        .map_err(|_| sample_error("process-group count exceeds platform capacity"))?;
    if count >= capacity {
        return Err(sample_error("process-group observation exceeded its bounded buffer"));
    }
    Ok(Some(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libproc_zero_with_error_is_unavailable_not_a_sampled_zero() {
        assert_eq!(
            observe_group(4, || {
                Errno::EIO.set();
                0
            })
            .expect("optional observations can be unavailable"),
            None
        );
    }

    #[test]
    fn libproc_query_clears_stale_errno_before_observing_empty_group() {
        Errno::EBUSY.set();
        assert_eq!(
            observe_group(4, || {
                assert_eq!(Errno::last_raw(), 0, "clear errno before the system call");
                0
            })
            .expect("successful empty observation"),
            Some(0)
        );
    }

    #[test]
    fn libproc_counts_preserve_optional_failures_and_buffer_bounds() {
        for count in 0..4 {
            assert_eq!(observe_group(4, || count).expect("bounded"), usize::try_from(count).ok());
        }
        assert_eq!(observe_group(4, || -1).expect("unavailable"), None);
        assert_eq!(
            observe_group(4, || {
                Errno::EIO.set();
                1
            })
            .expect("positive successful results do not use errno"),
            Some(1)
        );
        for count in [4, 5] {
            assert!(observe_group(4, || count).is_err(), "invalid count {count}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(
        unsafe_code,
        reason = "native negative test declares an undersized libproc buffer without exceeding its allocation"
    )]
    fn libproc_native_failed_query_is_an_unavailable_optional_sample() {
        let mut pid = 0_i32;
        let mut raw_observation = (-1, 0);
        let count = observe_group(1, || {
            // SAFETY: this live integer is writable for more than the declared one byte. The
            // too-small size deliberately requests an error; the query transfers no ownership.
            let count =
                unsafe { libc::proc_listpgrppids(1, (&raw mut pid).cast::<std::ffi::c_void>(), 1) };
            raw_observation = (count, Errno::last_raw());
            count
        })
        .expect("optional sample can be unavailable");
        assert_eq!(raw_observation.0, 0);
        assert_ne!(raw_observation.1, 0);
        assert_eq!(count, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn libproc_native_live_group_has_a_nonzero_process_count() {
        let group = u32::try_from(nix::unistd::getpgrp().as_raw()).expect("positive native group");
        let identity = crate::ProcessTreeIdentity::new(std::process::id(), None, Some(group), true);
        assert!(
            process_group_count(identity).expect("native sample").is_some_and(|count| count > 0)
        );
    }
}
