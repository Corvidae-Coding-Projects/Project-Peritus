//! Bounded live output, separate from the stable launch-evidence wire format.

use super::invalid;
use crate::{AppProtocolError, ControlOperationId, WorkbenchQuery, WorkbenchResultPage};

/// Maximum UTF-8 bytes exposed per preview stream in one observation.
pub const MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES: usize = 8192;
/// Maximum bytes returned by one exact preview-output range request.
pub const MAX_WORKBENCH_PREVIEW_OUTPUT_RANGE_BYTES: usize = 64 * 1024;

/// One native output stream exposed by a preview process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchPreviewOutputStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
    /// Combined interactive terminal output.
    Terminal,
}

/// Exact bounded range request into one authorized preview stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchPreviewOutputQuery {
    query: WorkbenchQuery,
    run: peritus_types::RunId,
    launch: ControlOperationId,
    stream: WorkbenchPreviewOutputStream,
    offset: u64,
    maximum_bytes: usize,
}

impl WorkbenchPreviewOutputQuery {
    /// Creates a bounded exact-process range request.
    ///
    /// # Errors
    /// Rejects zero or oversized chunks.
    pub const fn new(
        query: WorkbenchQuery,
        run: peritus_types::RunId,
        launch: ControlOperationId,
        stream: WorkbenchPreviewOutputStream,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<Self, AppProtocolError> {
        if maximum_bytes == 0 || maximum_bytes > MAX_WORKBENCH_PREVIEW_OUTPUT_RANGE_BYTES {
            return Err(invalid());
        }
        Ok(Self { query, run, launch, stream, offset, maximum_bytes })
    }
    /// Conversation and workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Governing run.
    #[must_use]
    pub const fn run(self) -> peritus_types::RunId {
        self.run
    }
    /// Exact launch identity.
    #[must_use]
    pub const fn launch(self) -> ControlOperationId {
        self.launch
    }
    /// Requested stream.
    #[must_use]
    pub const fn stream(self) -> WorkbenchPreviewOutputStream {
        self.stream
    }
    /// Requested starting byte offset.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
    /// Maximum response byte count.
    #[must_use]
    pub const fn maximum_bytes(self) -> usize {
        self.maximum_bytes
    }
}

/// One byte range from a preview process's durable stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchPreviewOutputRange {
    launch: ControlOperationId,
    stream: WorkbenchPreviewOutputStream,
    offset: u64,
    total_bytes: u64,
    artifact_digest: Option<[u8; 32]>,
    bytes: Vec<u8>,
}

impl WorkbenchPreviewOutputRange {
    /// Constructs one exact range and optional immutable terminal-artifact digest.
    ///
    /// # Errors
    /// Rejects oversized, empty nonterminal, or out-of-range responses.
    pub fn new(
        launch: ControlOperationId,
        stream: WorkbenchPreviewOutputStream,
        offset: u64,
        total_bytes: u64,
        artifact_digest: Option<[u8; 32]>,
        bytes: Vec<u8>,
    ) -> Result<Self, AppProtocolError> {
        let end = offset
            .checked_add(u64::try_from(bytes.len()).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        if bytes.len() > MAX_WORKBENCH_PREVIEW_OUTPUT_RANGE_BYTES
            || end > total_bytes
            || (offset < total_bytes && bytes.is_empty())
        {
            return Err(invalid());
        }
        Ok(Self { launch, stream, offset, total_bytes, artifact_digest, bytes })
    }
    /// Exact launch identity.
    #[must_use]
    pub const fn launch(&self) -> ControlOperationId {
        self.launch
    }
    /// Stream represented by this range.
    #[must_use]
    pub const fn stream(&self) -> WorkbenchPreviewOutputStream {
        self.stream
    }
    /// Starting byte offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Current or final retained stream size.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    /// Content digest for a finalized artifact, if the process is terminal.
    #[must_use]
    pub const fn artifact_digest(&self) -> Option<[u8; 32]> {
        self.artifact_digest
    }
    /// Exact requested byte range.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Retained tail of one authorized launch's output streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchPreviewOutput {
    launch: ControlOperationId,
    stdout: String,
    stderr: String,
    truncated: bool,
}
impl WorkbenchPreviewOutput {
    /// Constructs bounded output. Truncation is explicit and never implies complete evidence.
    ///
    /// # Errors
    /// Rejects either stream exceeding the public byte bound.
    pub fn new(
        launch: ControlOperationId,
        stdout: String,
        stderr: String,
        truncated: bool,
    ) -> Result<Self, AppProtocolError> {
        if stdout.len() > MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES
            || stderr.len() > MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES
        {
            return Err(invalid());
        }
        Ok(Self { launch, stdout, stderr, truncated })
    }
    /// Exact launch identity.
    #[must_use]
    pub const fn launch(&self) -> ControlOperationId {
        self.launch
    }
    /// Standard output, or combined PTY output for an interactive launch.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
    /// Separate standard error when the process uses pipes.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
    /// Whether earlier bytes are omitted from this observation.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Authenticated launch evidence and its bounded retained output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchPreviewSnapshot {
    result: WorkbenchResultPage,
    outputs: Vec<WorkbenchPreviewOutput>,
}
impl WorkbenchPreviewSnapshot {
    /// Binds output to unique launches present in the accompanying result page.
    ///
    /// # Errors
    /// Rejects duplicate or unbound output rows.
    pub fn new(
        result: WorkbenchResultPage,
        outputs: Vec<WorkbenchPreviewOutput>,
    ) -> Result<Self, AppProtocolError> {
        let mut seen = std::collections::BTreeSet::new();
        if outputs.iter().any(|output| {
            !seen.insert(output.launch())
                || !result.launches().iter().any(|launch| launch.launch() == output.launch())
        }) {
            return Err(invalid());
        }
        Ok(Self { result, outputs })
    }
    /// Existing launch and evidence projection.
    #[must_use]
    pub const fn result(&self) -> &WorkbenchResultPage {
        &self.result
    }
    /// Output scoped to launches in this page.
    #[must_use]
    pub fn outputs(&self) -> &[WorkbenchPreviewOutput] {
        &self.outputs
    }
}
