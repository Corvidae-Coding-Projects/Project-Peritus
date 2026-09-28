//! Read-only Windows host-process accounting; does not open or control other processes.

use std::io;
use windows_sys::Win32::System::{
    ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
    Threading::GetCurrentProcess,
};

/// Observes the calling Windows process's current resident working set in bytes.
///
/// This is a host observation, not an owned-child or process-tree resource measurement.
/// No subprocess, executable search, real process handle, or execution authority is required.
///
/// # Errors
/// Returns the operating-system query failure or an unrepresentable counter size/value.
#[allow(unsafe_code, reason = "C2 owns the narrow read-only Windows current-process query")]
pub fn current_process_resident_memory_bytes() -> io::Result<u64> {
    let size = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>())
        .map_err(|_| io::Error::other("process memory counter structure exceeds DWORD"))?;
    let mut counters = PROCESS_MEMORY_COUNTERS { cb: size, ..Default::default() };
    // SAFETY: GetCurrentProcess returns a valid pseudo handle with query access for this
    // calling process. It allocates no handle and must not be wrapped in an owning handle.
    // The initialized, aligned counters value is exclusively borrowed for this synchronous
    // call; the exact structure size bounds the writable buffer. Windows retains no pointer.
    // C2 / T-H0-009 trusted OS boundary; Miri cannot execute this Windows API. Native tests
    // exercise it without a shell search path.
    let success = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, size) };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(counters.WorkingSetSize)
        .map_err(|_| io::Error::other("process resident-memory count exceeds u64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_memory_is_observed_without_a_shell_search_path() {
        const CHILD: &str = "PERITUS_TEST_SELF_MEMORY_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
                .args([
                    "--exact",
                    "platform::self_memory::tests::current_process_memory_is_observed_without_a_shell_search_path",
                ])
                .env(CHILD, "1")
                .env("PATH", "")
                .output()
                .expect("isolated memory-query test");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed; 0 failed"),
                "{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        for _ in 0..32 {
            assert!(current_process_resident_memory_bytes().expect("current working set") > 0);
        }
    }
}
