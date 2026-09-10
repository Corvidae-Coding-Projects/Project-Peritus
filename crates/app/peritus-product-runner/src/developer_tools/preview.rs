//! Effect-free preview launch and observation values shared by both build modes.

use std::{path::PathBuf, time::Duration};

use peritus_types::ProcessId;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_PROGRAM_BYTES: usize = 4_096;
const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 64 * 1_024;
const MAX_ENVIRONMENT: usize = 64;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1_024;
const MAX_TIMEOUT: Duration = Duration::from_mins(10);

/// A direct executable launch admitted through the existing command and process gateways.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewCommand {
    pub(in crate::developer_tools) program: String,
    pub(in crate::developer_tools) arguments: Vec<String>,
    pub(in crate::developer_tools) cwd: PathBuf,
    pub(in crate::developer_tools) timeout: Duration,
    pub(in crate::developer_tools) interactive: bool,
    pub(in crate::developer_tools) rows: u16,
    pub(in crate::developer_tools) columns: u16,
    pub(in crate::developer_tools) idempotency_key: String,
    pub(in crate::developer_tools) environment: Vec<(String, String)>,
}

impl PreviewCommand {
    /// Validates conservative product bounds before any authorization or process effect.
    ///
    /// # Errors
    /// Rejects empty, oversized or NUL-containing executable, arguments, environment or limits.
    #[allow(clippy::too_many_arguments, reason = "each launch-policy input remains explicit")]
    pub fn new(
        program: String,
        arguments: Vec<String>,
        cwd: PathBuf,
        timeout: Duration,
        interactive: bool,
        rows: u16,
        columns: u16,
        idempotency_key: String,
        environment: Vec<(String, String)>,
    ) -> Result<Self, ProductRunnerError> {
        let invalid_program = program.is_empty()
            || program.len() > MAX_PROGRAM_BYTES
            || program.as_bytes().contains(&0);
        let invalid_arguments = arguments.len() > MAX_ARGUMENTS
            || arguments
                .iter()
                .any(|value| value.len() > MAX_ARGUMENT_BYTES || value.as_bytes().contains(&0));
        let invalid_environment = environment.len() > MAX_ENVIRONMENT
            || environment.iter().any(|(name, value)| {
                !valid_environment_name(name)
                    || value.len() > MAX_ENVIRONMENT_VALUE_BYTES
                    || value.as_bytes().contains(&0)
            });
        if invalid_program
            || invalid_arguments
            || invalid_environment
            || cwd.as_os_str().is_empty()
            || timeout.is_zero()
            || timeout > MAX_TIMEOUT
            || rows == 0
            || columns == 0
            || idempotency_key.is_empty()
            || idempotency_key.len() > 512
            || idempotency_key.as_bytes().contains(&0)
        {
            return Err(preview_error("preview launch profile is invalid or exceeds its bound"));
        }
        Ok(Self {
            program,
            arguments,
            cwd,
            timeout,
            interactive,
            rows,
            columns,
            idempotency_key,
            environment,
        })
    }
}

/// Exact C2 process identity and private runtime handle for one accepted preview launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewLaunch {
    pub(in crate::developer_tools) handle: String,
    pub(in crate::developer_tools) process_id: ProcessId,
}

impl PreviewLaunch {
    /// Returns the actual C2 process identity created by the reused process gateway.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId {
        self.process_id
    }
}

/// Truthful lifecycle projection; a running or cancelled process is never a passing check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewProcessState {
    /// The owned process is live.
    Running,
    /// The process completed with a successful terminal result.
    Succeeded,
    /// The process completed unsuccessfully.
    Failed,
    /// The owned process accepted cancellation and terminated.
    Cancelled,
    /// The immutable wall deadline expired.
    TimedOut,
    /// Recovery could not establish an exact outcome.
    Indeterminate,
}

/// Bounded user-visible process observation returned by the established command runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewObservation {
    pub(in crate::developer_tools) state: PreviewProcessState,
    pub(in crate::developer_tools) stdout: String,
    pub(in crate::developer_tools) stderr: String,
    pub(in crate::developer_tools) exit_code: Option<i64>,
    pub(in crate::developer_tools) progress: Vec<String>,
}

impl PreviewObservation {
    /// Returns the observed lifecycle state.
    #[must_use]
    pub const fn state(&self) -> PreviewProcessState {
        self.state
    }
    /// Borrows bounded retained standard output after terminal settlement.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
    /// Borrows bounded retained standard error after terminal settlement.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
    /// Returns an observed operating-system exit code when available.
    #[must_use]
    pub const fn exit_code(&self) -> Option<i64> {
        self.exit_code
    }
    /// Borrows ordered bounded process progress labels.
    #[must_use]
    pub fn progress(&self) -> &[String] {
        &self.progress
    }
}

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

fn preview_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Apply, "manage preview process", detail)
}
