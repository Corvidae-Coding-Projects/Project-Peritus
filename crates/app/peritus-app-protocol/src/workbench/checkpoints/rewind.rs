//! Exact revision-fenced rewind selection, preview, and per-path decisions.

use super::{
    AppProtocolError, CheckpointPathName, ControlOperationId, Sha256Digest,
    WorkbenchCheckpointVersion, WorkbenchQuery, invalid, valid_path, validate_lists,
};

/// Explicit rewind scope; logical branches never reuse live execution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchRewindMode {
    /// Restore only covered files and retain the current conversation.
    FilesOnly,
    /// Branch historical conversation context without changing current files.
    ConversationOnly,
    /// Publish the historical branch only after covered files settle successfully.
    Combined,
}

/// Exact checkpoint selection for a non-mutating rewind preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchRewindRequest {
    query: WorkbenchQuery,
    revision: u64,
    checkpoint: ControlOperationId,
    mode: WorkbenchRewindMode,
    child: Option<crate::ConversationId>,
    allocation: Option<crate::WorkbenchForkBudget>,
}
impl WorkbenchRewindRequest {
    /// Constructs a revision-fenced checkpoint selection.
    ///
    /// # Errors
    /// Rejects absent revision zero.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        checkpoint: ControlOperationId,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            Err(invalid())
        } else {
            Ok(Self {
                query,
                revision,
                checkpoint,
                mode: WorkbenchRewindMode::FilesOnly,
                child: None,
                allocation: None,
            })
        }
    }
    /// Selects a new logical branch, with a budget slice when the source has a governing goal.
    ///
    /// # Errors
    /// Rejects files-only mode and reusing the source identity.
    pub fn with_branch(
        mut self,
        mode: WorkbenchRewindMode,
        child: crate::ConversationId,
        allocation: Option<crate::WorkbenchForkBudget>,
    ) -> Result<Self, AppProtocolError> {
        if mode == WorkbenchRewindMode::FilesOnly || child == self.query.conversation() {
            return Err(invalid());
        }
        self.mode = mode;
        self.child = Some(child);
        self.allocation = allocation;
        Ok(self)
    }
    /// Returns the exact selected scope.
    #[must_use]
    pub const fn mode(self) -> WorkbenchRewindMode {
        self.mode
    }
    /// Returns the new same-workspace logical branch, if requested.
    #[must_use]
    pub const fn child(self) -> Option<crate::ConversationId> {
        self.child
    }
    /// Returns the visible governing budget reservation for that branch.
    #[must_use]
    pub const fn allocation(self) -> Option<crate::WorkbenchForkBudget> {
        self.allocation
    }
    /// Returns scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns inspected aggregate revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns checkpoint selection.
    #[must_use]
    pub const fn checkpoint(self) -> ControlOperationId {
        self.checkpoint
    }
}

/// Per-path rewind decision shown before confirmation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchRewindDisposition {
    /// Current bytes match the owned version and will be replaced or removed.
    Restore,
    /// Current bytes already match the checkpoint version; no write is needed.
    Unchanged,
    /// Current bytes differ from both safe known versions; no overwrite is allowed.
    Conflict,
    /// No completed owned boundary supplied an enforceable expected current version.
    Unsealed,
}

/// One exact target in a rewind preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchRewindPath {
    path: String,
    checkpoint: WorkbenchCheckpointVersion,
    expected_current: Option<WorkbenchCheckpointVersion>,
    observed_current: WorkbenchCheckpointVersion,
    disposition: WorkbenchRewindDisposition,
}
impl WorkbenchRewindPath {
    /// Constructs a validated exact target decision.
    ///
    /// # Errors
    /// Rejects malformed paths or inconsistent dispositions.
    pub fn new(
        path: String,
        checkpoint: WorkbenchCheckpointVersion,
        expected_current: Option<WorkbenchCheckpointVersion>,
        observed_current: WorkbenchCheckpointVersion,
        disposition: WorkbenchRewindDisposition,
    ) -> Result<Self, AppProtocolError> {
        if !valid_path(&path) {
            return Err(invalid());
        }
        let consistent = match disposition {
            WorkbenchRewindDisposition::Unchanged => observed_current == checkpoint,
            WorkbenchRewindDisposition::Restore => {
                expected_current == Some(observed_current) && observed_current != checkpoint
            }
            WorkbenchRewindDisposition::Conflict => expected_current.is_some_and(|expected| {
                observed_current != expected && observed_current != checkpoint
            }),
            WorkbenchRewindDisposition::Unsealed => expected_current.is_none(),
        };
        if !consistent {
            return Err(invalid());
        }
        Ok(Self { path, checkpoint, expected_current, observed_current, disposition })
    }
    /// Borrows path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns checkpoint version.
    #[must_use]
    pub const fn checkpoint(&self) -> WorkbenchCheckpointVersion {
        self.checkpoint
    }
    /// Returns completed owned version.
    #[must_use]
    pub const fn expected_current(&self) -> Option<WorkbenchCheckpointVersion> {
        self.expected_current
    }
    /// Returns version observed during preview.
    #[must_use]
    pub const fn observed_current(&self) -> WorkbenchCheckpointVersion {
        self.observed_current
    }
    /// Returns safe action classification.
    #[must_use]
    pub const fn disposition(&self) -> WorkbenchRewindDisposition {
        self.disposition
    }
}

/// Exact non-mutating restore preview. Re-encoding it binds confirmation to every fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchRewindPreview {
    request: WorkbenchRewindRequest,
    preview_digest: Sha256Digest,
    paths: Vec<WorkbenchRewindPath>,
    exclusions: Vec<String>,
    external_effects: Vec<String>,
    conversation_history_preserved: bool,
    accounting_preserved: bool,
}
impl WorkbenchRewindPreview {
    /// Constructs a bounded truthful preview of the explicitly selected scope.
    ///
    /// # Errors
    /// Rejects duplicate targets, count/text bounds, or preservation claims set false.
    pub fn new(
        request: WorkbenchRewindRequest,
        paths: Vec<WorkbenchRewindPath>,
        exclusions: Vec<String>,
        external_effects: Vec<String>,
    ) -> Result<Self, AppProtocolError> {
        if request.mode() == WorkbenchRewindMode::ConversationOnly && !paths.is_empty() {
            return Err(invalid());
        }
        validate_lists(request.revision(), &paths, &exclusions, &external_effects)?;
        let preview_digest = crate::wire::workbench_checkpoints::preview_fingerprint(
            request,
            &paths,
            &exclusions,
            &external_effects,
        )
        .map_err(|_| invalid())?;
        Ok(Self {
            request,
            preview_digest,
            paths,
            exclusions,
            external_effects,
            conversation_history_preserved: true,
            accounting_preserved: true,
        })
    }
    /// Returns original request.
    #[must_use]
    pub const fn request(&self) -> WorkbenchRewindRequest {
        self.request
    }
    /// Returns fingerprint over exact preview facts.
    #[must_use]
    pub const fn preview_digest(&self) -> Sha256Digest {
        self.preview_digest
    }
    /// Borrows target decisions.
    #[must_use]
    pub fn paths(&self) -> &[WorkbenchRewindPath] {
        &self.paths
    }
    /// Borrows visible checkpoint exclusions.
    #[must_use]
    pub fn exclusions(&self) -> &[String] {
        &self.exclusions
    }
    /// Borrows non-restorable external effects.
    #[must_use]
    pub fn external_effects(&self) -> &[String] {
        &self.external_effects
    }
    /// Reports that rewind appends a restore observation and retains original history.
    #[must_use]
    pub const fn conversation_history_preserved(&self) -> bool {
        self.conversation_history_preserved
    }
    /// Reports that rewind never resets governing cumulative accounting.
    #[must_use]
    pub const fn accounting_preserved(&self) -> bool {
        self.accounting_preserved
    }
}

impl CheckpointPathName for WorkbenchRewindPath {
    fn path_name(&self) -> &str {
        self.path()
    }
}
