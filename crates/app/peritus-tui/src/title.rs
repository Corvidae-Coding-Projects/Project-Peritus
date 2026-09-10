//! Scoped ownership of the Windows console title shared with foreground children.

use std::io;

#[cfg(windows)]
mod windows;

/// Restores the caller's Windows console title when interactive Peritus work ends.
///
/// Acquire before provider setup so child login commands cannot replace the saved shell title.
/// Nested owners restore the surrounding Peritus title. Other platforms and Windows processes
/// without a console are unaffected. Restoration on drop is best effort, like terminal teardown.
#[derive(Debug)]
pub struct TerminalTitle {
    #[cfg(windows)]
    previous: Option<windows::SavedTitle>,
}

impl TerminalTitle {
    /// Saves the current title and displays `Peritus` on an attached Windows console.
    ///
    /// # Errors
    /// Returns an OS error if an attached console's title cannot be read or changed.
    pub fn acquire() -> io::Result<Self> {
        let owner = Self {
            #[cfg(windows)]
            previous: windows::SavedTitle::capture()?,
        };
        owner.activate()?;
        Ok(owner)
    }

    /// Reasserts the Peritus title after returning from a foreground child command.
    ///
    /// # Errors
    /// Returns an OS error if the owned console's title cannot be changed.
    #[cfg(windows)]
    pub fn activate(&self) -> io::Result<()> {
        if self.previous.is_some() {
            return windows::SavedTitle::peritus().restore();
        }
        Ok(())
    }

    /// Leaves the terminal title unchanged on platforms without a Windows console.
    ///
    /// # Errors
    /// This platform's no-op always succeeds.
    #[cfg(not(windows))]
    pub const fn activate(&self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TerminalTitle {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(previous) = &self.previous {
            let _ = previous.restore();
        }
    }
}
