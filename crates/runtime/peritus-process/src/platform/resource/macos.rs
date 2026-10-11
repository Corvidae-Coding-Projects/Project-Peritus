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
