//! Byte ceilings and optional caller-selected subprocess duration.

use crate::ProviderCoreError;
use std::time::Duration;

/// Resource ceilings for one subprocess invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLimits {
    pub(super) max_stdin_bytes: usize,
    pub(super) max_stdout_bytes: usize,
    pub(super) max_stderr_bytes: usize,
    pub(super) timeout: Option<Duration>,
}

impl ProcessLimits {
    /// Production defaults for an account-runtime model turn.
    pub const PRODUCTION: Self = Self {
        max_stdin_bytes: 16 * 1024 * 1024,
        max_stdout_bytes: 16 * 1024 * 1024,
        max_stderr_bytes: 64 * 1024,
        timeout: None,
    };

    /// Creates nonzero byte and wall-clock ceilings.
    ///
    /// # Errors
    ///
    /// Rejects any zero ceiling.
    pub const fn new(
        max_stdin_bytes: usize,
        max_stdout_bytes: usize,
        max_stderr_bytes: usize,
        timeout: Duration,
    ) -> Result<Self, ProviderCoreError> {
        if max_stdin_bytes == 0
            || max_stdout_bytes == 0
            || max_stderr_bytes == 0
            || timeout.is_zero()
        {
            return Err(ProviderCoreError::configuration(
                "process_limits",
                "process limits must be nonzero",
            ));
        }
        Ok(Self { max_stdin_bytes, max_stdout_bytes, max_stderr_bytes, timeout: Some(timeout) })
    }

    /// Creates byte ceilings without an imposed execution deadline.
    ///
    /// # Errors
    /// Rejects zero byte ceilings.
    pub const fn without_deadline(
        max_stdin_bytes: usize,
        max_stdout_bytes: usize,
        max_stderr_bytes: usize,
    ) -> Result<Self, ProviderCoreError> {
        if max_stdin_bytes == 0 || max_stdout_bytes == 0 || max_stderr_bytes == 0 {
            return Err(ProviderCoreError::configuration(
                "process_limits",
                "process byte limits must be nonzero",
            ));
        }
        Ok(Self { max_stdin_bytes, max_stdout_bytes, max_stderr_bytes, timeout: None })
    }

    pub(crate) const fn max_stdout_bytes(self) -> usize {
        self.max_stdout_bytes
    }

    pub(crate) const fn max_stderr_bytes(self) -> usize {
        self.max_stderr_bytes
    }

    pub(crate) const fn timeout(self) -> Option<Duration> {
        self.timeout
    }
}
