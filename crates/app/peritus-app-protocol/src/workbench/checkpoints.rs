//! Public bounded checkpoint coverage, rewind previews, and durable restore receipts.

use crate::{AppErrorCode, AppProtocolError, ControlOperationId, WorkbenchQuery};
use peritus_types::Sha256Digest;

/// Maximum checkpoint name length.
pub const MAX_WORKBENCH_CHECKPOINT_NAME_BYTES: usize = 256;

/// Checked user-selected checkpoint name.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchCheckpointName(String);
impl WorkbenchCheckpointName {
    /// Validates a nonempty inert name.
    ///
    /// # Errors
    /// Rejects empty, oversized, or control-containing names.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.trim().is_empty()
            || value.len() > MAX_WORKBENCH_CHECKPOINT_NAME_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self(value))
    }
    /// Borrows exact name text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for WorkbenchCheckpointName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkbenchCheckpointName")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Portable mode retained in public coverage without platform-specific permission details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchCheckpointFileMode {
    /// Regular non-executable file.
    Regular,
    /// Regular executable file.
    Executable,
}

/// Exact covered file version; absence supports restoring deleted or newly created files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchCheckpointVersion {
    /// Path was absent.
    Absent,
    /// Path had exact bytes and portable mode.
    Present {
        /// Complete content digest.
        digest: Sha256Digest,
        /// Complete byte length.
        bytes: u64,
        /// Portable mode.
        mode: WorkbenchCheckpointFileMode,
    },
}

/// Stable source references retained for a later conversation fork without restoring them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointReferences {
    source_conversation_revision: u64,
    context_generation: u64,
    brief_revision: u64,
    goal_revision: Option<u64>,
}
impl WorkbenchCheckpointReferences {
    /// Constructs explicit historical bindings.
    #[must_use]
    pub const fn new(
        source_conversation_revision: u64,
        context_generation: u64,
        brief_revision: u64,
        goal_revision: Option<u64>,
    ) -> Self {
        Self { source_conversation_revision, context_generation, brief_revision, goal_revision }
    }
    /// Returns original conversation revision.
    #[must_use]
    pub const fn source_conversation_revision(self) -> u64 {
        self.source_conversation_revision
    }
    /// Returns referenced context generation.
    #[must_use]
    pub const fn context_generation(self) -> u64 {
        self.context_generation
    }
    /// Returns referenced brief revision.
    #[must_use]
    pub const fn brief_revision(self) -> u64 {
        self.brief_revision
    }
    /// Returns referenced goal revision when present; rewind never restores it.
    #[must_use]
    pub const fn goal_revision(self) -> Option<u64> {
        self.goal_revision
    }
}

/// One exact covered target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointPath {
    path: String,
    checkpoint: WorkbenchCheckpointVersion,
    expected_current: Option<WorkbenchCheckpointVersion>,
}
impl WorkbenchCheckpointPath {
    /// Validates one canonical relative target.
    ///
    /// # Errors
    /// Rejects malformed paths.
    pub fn new(
        path: String,
        checkpoint: WorkbenchCheckpointVersion,
        expected_current: Option<WorkbenchCheckpointVersion>,
    ) -> Result<Self, AppProtocolError> {
        if !valid_path(&path) {
            return Err(invalid());
        }
        Ok(Self { path, checkpoint, expected_current })
    }
    /// Borrows canonical relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns version captured for restore.
    #[must_use]
    pub const fn checkpoint(&self) -> WorkbenchCheckpointVersion {
        self.checkpoint
    }
    /// Returns completed owned post-change version when sealed.
    #[must_use]
    pub const fn expected_current(&self) -> Option<WorkbenchCheckpointVersion> {
        self.expected_current
    }
}

/// Durable checkpoint publication with visible coverage and exclusions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointReceipt {
    checkpoint: ControlOperationId,
    query: WorkbenchQuery,
    accepted_revision: u64,
    name: WorkbenchCheckpointName,
    references: WorkbenchCheckpointReferences,
    paths: Vec<WorkbenchCheckpointPath>,
    exclusions: Vec<String>,
    external_effects: Vec<String>,
}
impl WorkbenchCheckpointReceipt {
    /// Constructs a wire-representable receipt.
    ///
    /// # Errors
    /// Rejects zero revision, duplicate targets, or count/text bounds.
    pub fn new(
        checkpoint: ControlOperationId,
        query: WorkbenchQuery,
        accepted_revision: u64,
        name: WorkbenchCheckpointName,
        references: WorkbenchCheckpointReferences,
        paths: Vec<WorkbenchCheckpointPath>,
        exclusions: Vec<String>,
        external_effects: Vec<String>,
    ) -> Result<Self, AppProtocolError> {
        validate_lists(accepted_revision, &paths, &exclusions, &external_effects)?;
        Ok(Self {
            checkpoint,
            query,
            accepted_revision,
            name,
            references,
            paths,
            exclusions,
            external_effects,
        })
    }
    /// Returns checkpoint identity.
    #[must_use]
    pub const fn checkpoint(&self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns exact conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns accepted aggregate revision.
    #[must_use]
    pub const fn accepted_revision(&self) -> u64 {
        self.accepted_revision
    }
    /// Borrows name.
    #[must_use]
    pub const fn name(&self) -> &WorkbenchCheckpointName {
        &self.name
    }
    /// Returns historical references.
    #[must_use]
    pub const fn references(&self) -> WorkbenchCheckpointReferences {
        self.references
    }
    /// Borrows exact coverage.
    #[must_use]
    pub fn paths(&self) -> &[WorkbenchCheckpointPath] {
        &self.paths
    }
    /// Borrows visible exclusions.
    #[must_use]
    pub fn exclusions(&self) -> &[String] {
        &self.exclusions
    }
    /// Borrows effects which cannot be restored.
    #[must_use]
    pub fn external_effects(&self) -> &[String] {
        &self.external_effects
    }
    /// Computes the page-independent fingerprint over the complete retained manifest.
    ///
    /// # Errors
    /// Rejects a fact that cannot be represented by the canonical fingerprint encoding.
    pub fn coverage_fingerprint(&self) -> Result<Sha256Digest, peritus_codec::CodecError> {
        crate::wire::workbench_checkpoints::checkpoint_fingerprint(self)
    }

    /// Computes the paging binding over complete coverage and one selected revision.
    ///
    /// # Errors
    /// Rejects a fact that cannot be represented by the canonical fingerprint encoding.
    pub fn page_fingerprint(
        &self,
        selected_revision: u64,
    ) -> Result<Sha256Digest, peritus_codec::CodecError> {
        crate::wire::workbench_checkpoints::checkpoint_page_fingerprint(self, selected_revision)
    }
}

mod restore;
pub use restore::{WorkbenchRestoreReceipt, WorkbenchRestoreStatus, WorkbenchRestoreSummary};

mod rewind;
pub use rewind::{
    WorkbenchRewindConfirmation, WorkbenchRewindDisposition, WorkbenchRewindMode,
    WorkbenchRewindPath, WorkbenchRewindPreview, WorkbenchRewindRequest,
};

mod paging;
pub use paging::{
    WorkbenchCheckpointCoveragePage, WorkbenchCheckpointPageRequest, WorkbenchCoverageCursor,
    WorkbenchCoverageSection, WorkbenchRewindCoveragePage, WorkbenchRewindPageRequest,
};

fn validate_lists<T>(
    revision: u64,
    paths: &[T],
    exclusions: &[String],
    external_effects: &[String],
) -> Result<(), AppProtocolError>
where
    T: CheckpointPathName,
{
    if revision == 0
        || paths.windows(2).any(|pair| pair[0].path_name() >= pair[1].path_name())
        || exclusions
            .iter()
            .any(|text| text.is_empty() || text.len() > 4_610 || text.chars().any(char::is_control))
        || external_effects
            .iter()
            .any(|text| text.is_empty() || text.len() > 512 || text.chars().any(char::is_control))
    {
        Err(invalid())
    } else {
        Ok(())
    }
}

trait CheckpointPathName {
    fn path_name(&self) -> &str;
}
impl CheckpointPathName for WorkbenchCheckpointPath {
    fn path_name(&self) -> &str {
        self.path()
    }
}

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

#[cfg(test)]
mod tests;
