//! Exact UTF-16 console-title capture and restoration at the G2 presentation boundary.

use std::io;

use windows_sys::Win32::{
    Foundation::{ERROR_INVALID_HANDLE, ERROR_SUCCESS, GetLastError, SetLastError},
    System::Console::{GetConsoleCP, GetConsoleTitleW, SetConsoleTitleW},
};

// Windows limits console titles to less than 64K. Retain the terminating zero as well.
const TITLE_CAPACITY: u32 = 65_536;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct SavedTitle(Vec<u16>);

impl SavedTitle {
    #[allow(
        unsafe_code,
        reason = "G2 reads a bounded owned buffer through the Windows console API"
    )]
    pub(super) fn capture() -> io::Result<Option<Self>> {
        let mut title = vec![0_u16; TITLE_CAPACITY as usize];
        // SAFETY: The buffer owns TITLE_CAPACITY initialized writable UTF-16 units. Windows
        // borrows it only for this call. GetConsoleCP takes no pointers; last-error access is
        // confined to this OS thread. A detached process has no console input code page.
        let (length, error) = unsafe {
            if GetConsoleCP() == 0 {
                return Ok(None);
            }
            SetLastError(ERROR_SUCCESS);
            let length = GetConsoleTitleW(title.as_mut_ptr(), TITLE_CAPACITY);
            (length, GetLastError())
        };
        if length == 0 && error != ERROR_SUCCESS {
            return if error == ERROR_INVALID_HANDLE {
                Ok(None)
            } else {
                Err(io::Error::last_os_error())
            };
        }
        title.truncate(length as usize + 1);
        Ok(Some(Self(title)))
    }

    pub(super) fn peritus() -> Self {
        Self("Peritus".encode_utf16().chain(Some(0)).collect())
    }

    #[allow(
        unsafe_code,
        reason = "G2 restores its owned UTF-16 title through the Windows console API"
    )]
    pub(super) fn restore(&self) -> io::Result<()> {
        // SAFETY: Both constructors retain an initialized, zero-terminated UTF-16 buffer.
        // Windows reads it synchronously and does not retain the pointer.
        if unsafe { SetConsoleTitleW(self.0.as_ptr()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
