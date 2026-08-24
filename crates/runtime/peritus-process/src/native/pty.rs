//! Target-side PTY attachment kept separate from helper protocol pipes.

/// Reserved helper environment key carrying a process-owned Unix PTY slave device.
///
/// The process launcher clears the helper environment, overwrites this value after plan
/// projection, and [`NativePtyAttachment::configure`] removes it from the literal target command.
pub const NATIVE_PTY_SLAVE_ENV: &str = "PERITUS_NATIVE_PTY_SLAVE_V1";

/// Open target-side PTY attachment kept separate from native helper protocol pipes.
pub struct NativePtyAttachment {
    slave: std::fs::File,
}

impl NativePtyAttachment {
    /// Opens the process-owned PTY slave named by the reserved helper environment.
    ///
    /// Helpers call this before installing filesystem restrictions, retain the open handle while
    /// exchanging the activation protocol on ordinary pipes, and configure the literal target
    /// command only after acknowledging complete native activation.
    ///
    /// # Errors
    /// Returns the operating-system open failure for a C2-supplied PTY device.
    pub fn from_environment() -> std::io::Result<Option<Self>> {
        let Some(path) = std::env::var_os(NATIVE_PTY_SLAVE_ENV) else {
            return Ok(None);
        };
        let slave = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Some(Self { slave }))
    }

    /// Redirects the literal target's standard streams to the retained PTY slave.
    ///
    /// # Errors
    /// Returns an error if either required duplicate of the retained PTY handle cannot be made.
    pub fn configure(self, command: &mut std::process::Command) -> std::io::Result<()> {
        let input = self.slave.try_clone()?;
        let output = self.slave.try_clone()?;
        command
            .env_remove(NATIVE_PTY_SLAVE_ENV)
            .stdin(std::process::Stdio::from(input))
            .stdout(std::process::Stdio::from(output))
            .stderr(std::process::Stdio::from(self.slave));
        Ok(())
    }
}

impl std::os::fd::AsFd for NativePtyAttachment {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(&self.slave)
    }
}

impl std::os::fd::AsRawFd for NativePtyAttachment {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self.slave)
    }
}
