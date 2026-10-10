//! Exact non-running conversation fork lineage.

use super::{
    AppErrorCode, AppProtocolError, ControlOperationId, ConversationTitle, WorkbenchQuery,
};

/// Workspace relationship for a child fork.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchForkMode {
    /// Current workspace is context only; execution admission is disabled.
    ReadOnlyCurrentWorkspace,
    /// Child is bound to a distinct registered workspace.
    IsolatedWritableWorkspace,
}

/// Exact user-selected lineage and target for a non-running child.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchForkRequest {
    child: WorkbenchQuery,
    title: ConversationTitle,
    checkpoint: ControlOperationId,
    source_revision: u64,
    context_generation: u64,
    brief_revision: u64,
    goal_revision: u64,
    mode: WorkbenchForkMode,
}
impl WorkbenchForkRequest {
    /// Creates an exact branch request. The daemon separately checks parent and workspace authority.
    ///
    /// # Errors
    /// Rejects an absent source revision.
    #[allow(clippy::too_many_arguments, reason = "checkpoint lineage remains explicit")]
    pub fn new(
        child: WorkbenchQuery,
        title: ConversationTitle,
        checkpoint: ControlOperationId,
        source_revision: u64,
        context_generation: u64,
        brief_revision: u64,
        goal_revision: u64,
        mode: WorkbenchForkMode,
    ) -> Result<Self, AppProtocolError> {
        if source_revision == 0 {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self {
            child,
            title,
            checkpoint,
            source_revision,
            context_generation,
            brief_revision,
            goal_revision,
            mode,
        })
    }
    /// Returns the independent child scope.
    #[must_use]
    pub const fn child(&self) -> WorkbenchQuery {
        self.child
    }
    /// Borrows the child title.
    #[must_use]
    pub const fn title(&self) -> &ConversationTitle {
        &self.title
    }
    /// Returns checkpoint creation-operation identity.
    #[must_use]
    pub const fn checkpoint(&self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns exact source conversation revision.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    /// Returns source context generation.
    #[must_use]
    pub const fn context_generation(&self) -> u64 {
        self.context_generation
    }
    /// Returns source brief revision, or zero when absent.
    #[must_use]
    pub const fn brief_revision(&self) -> u64 {
        self.brief_revision
    }
    /// Returns source goal revision, or zero when absent.
    #[must_use]
    pub const fn goal_revision(&self) -> u64 {
        self.goal_revision
    }
    /// Returns selected workspace mode.
    #[must_use]
    pub const fn mode(&self) -> WorkbenchForkMode {
        self.mode
    }
}
