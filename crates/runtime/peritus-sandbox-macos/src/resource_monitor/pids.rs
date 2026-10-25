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

#[cfg(target_os = "macos")]
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
