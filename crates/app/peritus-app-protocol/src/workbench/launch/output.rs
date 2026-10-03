//! Bounded live output, separate from the stable launch-evidence wire format.

use super::invalid;
use crate::{AppProtocolError, ControlOperationId, WorkbenchResultPage};

/// Maximum UTF-8 bytes exposed per preview stream in one observation.
pub const MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES: usize = 8192;

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
