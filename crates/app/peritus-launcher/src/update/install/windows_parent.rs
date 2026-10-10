//! Binds deferred installation to the launcher's native process creation identity.
#![allow(unsafe_code)]

use crate::LauncherError;
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetCurrentProcess, GetProcessTimes},
};

#[derive(Clone, Copy)]
pub(super) struct ParentIdentity {
    pub(super) id: u32,
    pub(super) created: u64,
}
impl ParentIdentity {
    pub(super) fn current() -> Result<Self, LauncherError> {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: the pseudo-handle denotes this live process; each output points to a valid,
        // exclusive FILETIME for the duration of the native query. The pseudo-handle is not owned.
        let observed = unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &raw mut created,
                &raw mut exited,
                &raw mut kernel,
                &raw mut user,
            )
        };
        if observed == 0 {
            return Err(LauncherError::Update(format!(
                "read updater parent creation identity: {}",
                std::io::Error::last_os_error()
            )));
        }
        let created = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Ok(Self { id: std::process::id(), created })
    }
}
