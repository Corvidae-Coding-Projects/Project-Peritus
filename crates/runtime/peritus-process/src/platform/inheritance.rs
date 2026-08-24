//! Exact protected-handle inheritance for the direct native helper child.

use std::process::Command;

use crate::{ErrorCode, NativeProtectedHandle, ProcessError, ProcessOperation, RecoveryClass};

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "pre-exec fcntl is the narrow Unix boundary that enables only admitted child handles"
)]
pub(crate) fn configure_protected_inheritance(
    command: &mut Command,
    handles: &[NativeProtectedHandle],
) -> Result<InheritanceGuard, ProcessError> {
    use std::os::unix::process::CommandExt;

    let descriptors = handles
        .iter()
        .map(|handle| {
            i32::try_from(handle.raw_handle())
                .map_err(|_| inheritance_error("native protected descriptor is invalid"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // SAFETY: the closure performs only async-signal-safe `fcntl` operations over live descriptors
    // retained by `NativeLaunchDescription`. It mutates only the forked child copy immediately
    // before exec, so the parent's close-on-exec state and concurrent launches are unaffected.
    unsafe {
        command.pre_exec(move || {
            for descriptor in &descriptors {
                let flags = libc::fcntl(*descriptor, libc::F_GETFD);
                if flags < 0
                    || libc::fcntl(*descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(InheritanceGuard {})
}

/// No parent-global state is changed by the Unix pre-exec adapter.
#[cfg(unix)]
pub(crate) struct InheritanceGuard {}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "SetHandleInformation is the narrow Windows inherited-handle launch boundary"
)]
pub(crate) fn configure_protected_inheritance(
    _command: &mut Command,
    handles: &[NativeProtectedHandle],
) -> Result<InheritanceGuard, ProcessError> {
    use std::sync::{Mutex, OnceLock};
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    static INHERITANCE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = INHERITANCE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| inheritance_error("native protected handle launch lock was poisoned"))?;
    let mut enabled = Vec::with_capacity(handles.len());
    for handle in handles {
        let raw = usize::try_from(handle.raw_handle())
            .map_err(|_| inheritance_error("native protected Windows handle is invalid"))?
            as *mut core::ffi::c_void;
        // SAFETY: each raw value belongs to a live `File` retained by the launch description. The
        // process-wide lock serializes Peritus child creation until `Drop` restores every flag.
        if unsafe { SetHandleInformation(raw, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            for prior in enabled {
                // SAFETY: each prior value was enabled successfully in this locked transaction.
                let _ = unsafe { SetHandleInformation(prior, HANDLE_FLAG_INHERIT, 0) };
            }
            return Err(inheritance_error("native protected Windows handle cannot be inherited"));
        }
        enabled.push(raw);
    }
    Ok(InheritanceGuard { _lock: lock, enabled })
}

#[cfg(windows)]
pub(crate) struct InheritanceGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    enabled: Vec<*mut core::ffi::c_void>,
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "paired restoration for the narrow Windows inherited-handle launch boundary"
)]
impl Drop for InheritanceGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

        for handle in &self.enabled {
            // SAFETY: these exact live handles were enabled while holding `_lock`; the guard is
            // dropped immediately after spawn and restores the non-inheritable parent state.
            let _ = unsafe { SetHandleInformation(*handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

const fn inheritance_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Spawn,
        ProcessOperation::Spawn,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
