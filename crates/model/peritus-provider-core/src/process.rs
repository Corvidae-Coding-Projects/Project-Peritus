//! Bounded owned subprocess requests with a runtime-private Tokio implementation.

mod tokio_transport;

use core::fmt;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use zeroize::Zeroizing;

use crate::{BoxFuture, CancellationToken, ProviderCoreError};

pub use tokio_transport::TokioProcessTransport;

const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENVIRONMENT_REMOVALS: usize = 128;

/// A canonical, immutable executable path.
#[derive(Clone, Eq, PartialEq)]
pub struct ProcessExecutable(PathBuf);

impl ProcessExecutable {
    /// Canonicalizes and pins a regular executable file.
    ///
    /// # Errors
    ///
    /// Rejects paths that cannot be canonicalized or do not name a regular file.
    pub fn pin(path: impl AsRef<Path>) -> Result<Self, ProviderCoreError> {
        let path = std::fs::canonicalize(path).map_err(|_| {
            ProviderCoreError::configuration(
                "process_executable",
                "executable path could not be canonicalized",
            )
        })?;
        if !path.is_file() {
            return Err(ProviderCoreError::configuration(
                "process_executable",
                "executable path is not a regular file",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            if std::fs::metadata(&path)
                .map_err(|_| {
                    ProviderCoreError::configuration(
                        "process_executable",
                        "executable metadata could not be read",
                    )
                })?
                .permissions()
                .mode()
                & 0o111
                == 0
            {
                return Err(ProviderCoreError::configuration(
                    "process_executable",
                    "executable path is not marked executable",
                ));
            }
        }
        Ok(Self(path))
    }

    /// Returns the pinned path for process creation.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Debug for ProcessExecutable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProcessExecutable([pinned])")
    }
}

/// Checked environment-variable name removed from a child process.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentName(String);

impl EnvironmentName {
    /// Creates an ASCII environment-variable name.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or non-identifier names.
    pub fn new(value: String) -> Result<Self, ProviderCoreError> {
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ProviderCoreError::configuration(
                "process_environment",
                "environment name is empty, invalid, or exceeds its bound",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the checked name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resource ceilings for one subprocess invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLimits {
    max_stdin_bytes: usize,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
    timeout: Duration,
}

impl ProcessLimits {
    /// Production defaults for an account-runtime model turn.
    pub const PRODUCTION: Self = Self {
        max_stdin_bytes: 16 * 1024 * 1024,
        max_stdout_bytes: 16 * 1024 * 1024,
        max_stderr_bytes: 64 * 1024,
        timeout: Duration::from_mins(10),
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
        Ok(Self { max_stdin_bytes, max_stdout_bytes, max_stderr_bytes, timeout })
    }

    pub(crate) const fn max_stdout_bytes(self) -> usize {
        self.max_stdout_bytes
    }

    pub(crate) const fn max_stderr_bytes(self) -> usize {
        self.max_stderr_bytes
    }

    pub(crate) const fn timeout(self) -> Duration {
        self.timeout
    }
}

/// One explicit, redacted subprocess invocation.
pub struct ProcessRequest {
    executable: ProcessExecutable,
    arguments: Vec<String>,
    stdin: Zeroizing<Vec<u8>>,
    current_dir: Option<PathBuf>,
    environment_removals: Vec<EnvironmentName>,
    limits: ProcessLimits,
}

impl ProcessRequest {
    /// Creates a bounded request with explicit argv, stdin, cwd, and environment removals.
    ///
    /// # Errors
    ///
    /// Rejects oversized/NUL-containing argv or stdin, duplicate environment names, excessive
    /// removals, or a configured current directory that is not a directory.
    pub fn new(
        executable: ProcessExecutable,
        arguments: Vec<String>,
        stdin: Vec<u8>,
        current_dir: Option<PathBuf>,
        environment_removals: Vec<EnvironmentName>,
        limits: ProcessLimits,
    ) -> Result<Self, ProviderCoreError> {
        let argument_bytes =
            arguments.iter().try_fold(0_usize, |total, argument| total.checked_add(argument.len()));
        if arguments.len() > MAX_ARGUMENTS
            || argument_bytes.is_none_or(|bytes| bytes > MAX_ARGUMENT_BYTES)
            || arguments.iter().any(|argument| argument.as_bytes().contains(&0))
        {
            return Err(ProviderCoreError::limit_exceeded(
                "process_request",
                "process arguments are invalid or exceed their bound",
            ));
        }
        if stdin.len() > limits.max_stdin_bytes {
            return Err(ProviderCoreError::limit_exceeded(
                "process_request",
                "process stdin exceeds its bound",
            ));
        }
        if current_dir.as_ref().is_some_and(|path| !path.is_dir()) {
            return Err(ProviderCoreError::configuration(
                "process_request",
                "process current directory is not a directory",
            ));
        }
        if environment_removals.len() > MAX_ENVIRONMENT_REMOVALS
            || environment_removals.iter().collect::<BTreeSet<_>>().len()
                != environment_removals.len()
        {
            return Err(ProviderCoreError::configuration(
                "process_request",
                "environment removals are duplicate or exceed their bound",
            ));
        }
        Ok(Self {
            executable,
            arguments,
            stdin: Zeroizing::new(stdin),
            current_dir,
            environment_removals,
            limits,
        })
    }

    /// Returns the pinned executable for a process transport implementation.
    #[must_use]
    pub const fn executable(&self) -> &ProcessExecutable {
        &self.executable
    }

    /// Borrows sensitive argv for a process transport implementation.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Borrows sensitive stdin for a process transport implementation.
    #[must_use]
    pub fn stdin(&self) -> &[u8] {
        &self.stdin
    }

    /// Returns the explicit child current directory.
    #[must_use]
    pub fn current_dir(&self) -> Option<&Path> {
        self.current_dir.as_deref()
    }

    /// Returns the complete environment-removal set.
    #[must_use]
    pub fn environment_removals(&self) -> &[EnvironmentName] {
        &self.environment_removals
    }

    /// Returns configured resource ceilings.
    #[must_use]
    pub const fn limits(&self) -> ProcessLimits {
        self.limits
    }
}

impl fmt::Debug for ProcessRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessRequest")
            .field("executable", &self.executable)
            .field("arguments", &"[redacted]")
            .field("stdin", &"[redacted]")
            .field("current_dir", &self.current_dir.as_ref().map(|_| "[configured]"))
            .field("environment_removals", &self.environment_removals)
            .field("limits", &self.limits)
            .finish()
    }
}

/// Stable child-process terminal status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessExit {
    success: bool,
    code: Option<i32>,
}

impl ProcessExit {
    /// Creates a stable exit observation for a process transport.
    #[must_use]
    pub const fn new(success: bool, code: Option<i32>) -> Self {
        Self { success, code }
    }

    /// Returns whether the executable reported success.
    #[must_use]
    pub const fn success(self) -> bool {
        self.success
    }

    /// Returns the portable numeric exit code when available.
    #[must_use]
    pub const fn code(self) -> Option<i32> {
        self.code
    }
}

/// Bounded subprocess output whose formatting never exposes child bytes.
pub struct ProcessOutput {
    exit: ProcessExit,
    stdout: Zeroizing<Vec<u8>>,
    stderr: Zeroizing<Vec<u8>>,
}

impl ProcessOutput {
    /// Creates checked output, primarily for alternate transports and hermetic fakes.
    ///
    /// # Errors
    ///
    /// Rejects stdout or stderr beyond the supplied process limits.
    pub fn new(
        exit: ProcessExit,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        limits: ProcessLimits,
    ) -> Result<Self, ProviderCoreError> {
        if stdout.len() > limits.max_stdout_bytes || stderr.len() > limits.max_stderr_bytes {
            return Err(ProviderCoreError::limit_exceeded(
                "process_output",
                "alternate transport output exceeded its byte limit",
            ));
        }
        Ok(Self { exit, stdout: Zeroizing::new(stdout), stderr: Zeroizing::new(stderr) })
    }

    /// Returns child terminal status.
    #[must_use]
    pub const fn exit(&self) -> ProcessExit {
        self.exit
    }

    /// Borrows bounded stdout for provider-specific decoding.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Borrows bounded stderr for redaction-safe classification only.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

impl fmt::Debug for ProcessOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessOutput")
            .field("exit", &self.exit)
            .field("stdout", &"[redacted]")
            .field("stderr", &"[redacted]")
            .finish()
    }
}

/// Owned subprocess effect seam used by account-runtime adapters and hermetic fakes.
pub trait ProcessTransport: Send + Sync {
    /// Runs one bounded process until exit, timeout, or cancellation.
    fn run<'a>(
        &'a self,
        request: ProcessRequest,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ProcessOutput, ProviderCoreError>>;
}
