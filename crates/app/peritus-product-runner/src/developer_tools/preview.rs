//! Effect-free preview launch and observation values shared by both build modes.

use std::{path::PathBuf, time::Duration};

use peritus_types::ProcessId;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// One bounded range from a preview output stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewOutputRange {
    pub(super) total_bytes: u64,
    pub(super) digest: Option<[u8; 32]>,
    pub(super) bytes: Vec<u8>,
}

/// Identifies the retained source searched for a preview output match.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum PreviewOutputStream {
    /// Pipe standard output.
    Stdout,
    /// Pipe standard error.
    Stderr,
    /// Combined PTY terminal stream.
    Terminal,
}

impl PreviewOutputStream {
    /// Converts to the process-runtime stream identifier.
    #[must_use]
    pub const fn process_stream(self) -> peritus_process::OutputStream {
        match self {
            Self::Stdout => peritus_process::OutputStream::Stdout,
            Self::Stderr => peritus_process::OutputStream::Stderr,
            Self::Terminal => peritus_process::OutputStream::Terminal,
        }
    }
}

impl From<peritus_process::OutputStream> for PreviewOutputStream {
    fn from(value: peritus_process::OutputStream) -> Self {
        match value {
            peritus_process::OutputStream::Stdout => Self::Stdout,
            peritus_process::OutputStream::Stderr => Self::Stderr,
            peritus_process::OutputStream::Terminal => Self::Terminal,
        }
    }
}

/// Identifies the retained source searched for a preview output match.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum PreviewOutputMatchSource {
    /// The output was observed in the exact live process spool.
    LiveSpool {
        /// Exact owned process identifier encoded as bytes.
        process_id: [u8; 16],
        /// SHA-256 of every byte in the observed spool prefix.
        observed_prefix_digest: [u8; 32],
    },
    /// The output was observed in the immutable artifact published at process termination.
    FinalizedArtifact {
        /// Exact owned process identifier encoded as bytes.
        process_id: [u8; 16],
        /// SHA-256 digest of the finalized output artifact.
        artifact_digest: [u8; 32],
    },
}

/// Exact byte evidence for a literal preview-output match.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewOutputMatch {
    pub(crate) stream: PreviewOutputStream,
    pub(crate) start_byte: u64,
    pub(crate) end_byte: u64,
    pub(crate) observed_stream_bytes: u64,
    pub(crate) matched_bytes_digest: [u8; 32],
    pub(crate) source: PreviewOutputMatchSource,
}

impl PreviewOutputMatch {
    /// Output stream containing the match.
    #[must_use]
    pub const fn stream(self) -> PreviewOutputStream {
        self.stream
    }

    /// Inclusive start byte offset of the match in the full stream.
    #[must_use]
    pub const fn start_byte(self) -> u64 {
        self.start_byte
    }

    /// Exclusive end byte offset of the match in the full stream.
    #[must_use]
    pub const fn end_byte(self) -> u64 {
        self.end_byte
    }

    /// Number of stream bytes observed by this search.
    #[must_use]
    pub const fn observed_stream_bytes(self) -> u64 {
        self.observed_stream_bytes
    }

    /// SHA-256 digest of the exact UTF-8 needle bytes that matched.
    #[must_use]
    pub const fn matched_bytes_digest(self) -> [u8; 32] {
        self.matched_bytes_digest
    }

    /// Live-spool or finalized-artifact identity for the searched source.
    #[must_use]
    pub const fn source(self) -> PreviewOutputMatchSource {
        self.source
    }

    /// Exact process identifier bytes bound to this observation.
    #[must_use]
    pub const fn process_id_bytes(self) -> [u8; 16] {
        match self.source {
            PreviewOutputMatchSource::LiveSpool { process_id, .. }
            | PreviewOutputMatchSource::FinalizedArtifact { process_id, .. } => process_id,
        }
    }

    /// Finalized output artifact digest, or `None` for a live spool observation.
    #[must_use]
    pub const fn artifact_digest(self) -> Option<[u8; 32]> {
        match self.source {
            PreviewOutputMatchSource::LiveSpool { .. } => None,
            PreviewOutputMatchSource::FinalizedArtifact { artifact_digest, .. } => {
                Some(artifact_digest)
            }
        }
    }

    /// SHA-256 digest binding all bytes in the observed source snapshot.
    #[must_use]
    pub const fn observed_source_digest(self) -> [u8; 32] {
        match self.source {
            PreviewOutputMatchSource::LiveSpool { observed_prefix_digest, .. } => {
                observed_prefix_digest
            }
            PreviewOutputMatchSource::FinalizedArtifact { artifact_digest, .. } => artifact_digest,
        }
    }

    /// Rejects malformed serialized match evidence before it is bound to a process record.
    ///
    /// # Errors
    /// Returns an error for an invalid process identifier or range outside the observed stream.
    pub const fn validate(self) -> Result<(), &'static str> {
        let process_id = self.process_id_bytes();
        if ProcessId::new(process_id).is_err() {
            return Err("preview output match process identifier is invalid");
        }
        if self.start_byte >= self.end_byte || self.end_byte > self.observed_stream_bytes {
            return Err("preview output match range is invalid");
        }
        Ok(())
    }

    /// Checks that this evidence binds the exact process and literal UTF-8 search text.
    ///
    /// # Errors
    /// Returns an error when the process, byte length, or needle digest does not match.
    pub fn validate_for_needle(
        self,
        process_id: ProcessId,
        needle: &str,
    ) -> Result<(), &'static str> {
        self.validate()?;
        if needle.is_empty()
            || self.process_id_bytes() != *process_id.as_bytes()
            || self.end_byte - self.start_byte != u64::try_from(needle.len()).unwrap_or(u64::MAX)
        {
            return Err("preview output match does not bind the requested process and needle");
        }
        let expected_digest: [u8; 32] = Sha256::digest(needle.as_bytes()).into();
        if self.matched_bytes_digest != expected_digest {
            return Err("preview output match digest differs from the requested needle");
        }
        Ok(())
    }
}

impl PreviewOutputRange {
    /// Current or finalized stream size.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    /// Final artifact digest, absent while the process is live.
    #[must_use]
    pub const fn digest(&self) -> Option<[u8; 32]> {
        self.digest
    }
    /// Exact requested byte range.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A direct executable launch admitted through the existing command and process gateways.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewCommand {
    pub(in crate::developer_tools) program: String,
    pub(in crate::developer_tools) arguments: Vec<String>,
    pub(in crate::developer_tools) cwd: PathBuf,
    pub(in crate::developer_tools) timeout: Option<Duration>,
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
        Self::new_optional(
            program,
            arguments,
            cwd,
            Some(timeout),
            interactive,
            rows,
            columns,
            idempotency_key,
            environment,
        )
    }

    /// Creates a preview command with an optional wall-clock timeout.
    #[allow(clippy::too_many_arguments, reason = "each launch-policy input remains explicit")]
    ///
    /// # Errors
    ///
    /// Returns an error when a command field violates native launch constraints.
    pub fn new_optional(
        program: String,
        arguments: Vec<String>,
        cwd: PathBuf,
        timeout: Option<Duration>,
        interactive: bool,
        rows: u16,
        columns: u16,
        idempotency_key: String,
        environment: Vec<(String, String)>,
    ) -> Result<Self, ProductRunnerError> {
        let invalid_program = program.is_empty() || program.as_bytes().contains(&0);
        let invalid_arguments = arguments.iter().any(|value| value.as_bytes().contains(&0));
        let invalid_environment = environment
            .iter()
            .any(|(name, value)| !valid_environment_name(name) || value.as_bytes().contains(&0));
        if invalid_program
            || invalid_arguments
            || invalid_environment
            || cwd.as_os_str().is_empty()
            || timeout.is_some_and(|value| value.is_zero())
            || timeout.is_some_and(|value| value.as_millis() > u128::from(u64::MAX))
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

/// Persistable observation identity for an owned preview. It grants no launch authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewOwner {
    source_run: peritus_types::RunId,
    execution_run: peritus_types::RunId,
    action: peritus_types::ActionId,
    process: ProcessId,
}

impl PreviewOwner {
    /// Reconstructs an untrusted saved binding; the runtime validates it against durable authority.
    #[must_use]
    pub const fn new(
        source_run: peritus_types::RunId,
        execution_run: peritus_types::RunId,
        action: peritus_types::ActionId,
        process: ProcessId,
    ) -> Self {
        Self { source_run, execution_run, action, process }
    }
    /// Returns the source runtime identity.
    #[must_use]
    pub const fn source_run(self) -> peritus_types::RunId {
        self.source_run
    }
    /// Returns the exact execution run.
    #[must_use]
    pub const fn execution_run(self) -> peritus_types::RunId {
        self.execution_run
    }
    /// Returns the one-use action identity.
    #[must_use]
    pub const fn action(self) -> peritus_types::ActionId {
        self.action
    }
    /// Returns the owned native process identity.
    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_command_accepts_a_positive_timeout_above_ten_minutes() {
        assert!(
            PreviewCommand::new(
                "cargo".to_owned(),
                vec!["test".to_owned()],
                PathBuf::from("."),
                Duration::from_secs(601),
                false,
                24,
                80,
                "long-preview".to_owned(),
                Vec::new(),
            )
            .is_ok()
        );
    }

    #[test]
    fn preview_command_accepts_collections_beyond_the_old_limits() {
        let arguments = (0..257).map(|index| format!("argument-{index}")).collect();
        let environment =
            (0..65).map(|index| (format!("VARIABLE_{index}"), index.to_string())).collect();

        let command = PreviewCommand::new(
            "cargo".to_owned(),
            arguments,
            PathBuf::from("."),
            Duration::from_secs(1),
            false,
            24,
            80,
            "large-preview-profile".to_owned(),
            environment,
        )
        .expect("protocol-representable preview profile");

        assert_eq!(command.arguments.len(), 257);
        assert_eq!(command.environment.len(), 65);
    }
}
