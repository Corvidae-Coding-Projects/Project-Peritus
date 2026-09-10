//! Exact non-running conversation fork lineage and optional child allocation.

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

/// Positive fixed child allocation reserved from the source goal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchForkBudget {
    active_millis: u64,
    requests: u32,
    tool_calls: u32,
    total_tokens: u64,
}
impl WorkbenchForkBudget {
    /// Validates all four allocation dimensions through the ordinary goal limits.
    ///
    /// # Errors
    /// Rejects zero fields or values outside the installed goal limits.
    pub fn new(
        active_millis: u64,
        requests: u32,
        tool_calls: u32,
        total_tokens: u64,
    ) -> Result<Self, AppProtocolError> {
        crate::WorkbenchGoalBudget::new(
            Some(active_millis),
            Some(requests),
            Some(tool_calls),
            Some(total_tokens),
        )?;
        Ok(Self { active_millis, requests, tool_calls, total_tokens })
    }
    /// Returns allocated active milliseconds.
    #[must_use]
    pub const fn active_millis(self) -> u64 {
        self.active_millis
    }
    /// Returns allocated provider requests.
    #[must_use]
    pub const fn requests(self) -> u32 {
        self.requests
    }
    /// Returns allocated tool calls.
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Returns allocated reported/derived tokens.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
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
    allocation: Option<WorkbenchForkBudget>,
}
impl WorkbenchForkRequest {
    /// Creates an exact branch request. The daemon separately checks parent and workspace authority.
    ///
    /// # Errors
    /// Rejects absent source revisions or an unallocated isolated writable branch.
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
        allocation: Option<WorkbenchForkBudget>,
    ) -> Result<Self, AppProtocolError> {
        if source_revision == 0
            || (mode == WorkbenchForkMode::IsolatedWritableWorkspace && allocation.is_none())
        {
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
            allocation,
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
    /// Returns the optional reserved child allocation.
    #[must_use]
    pub const fn allocation(&self) -> Option<WorkbenchForkBudget> {
        self.allocation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConversationId;
    use peritus_types::WorkspaceId;

    fn request(
        mode: WorkbenchForkMode,
        allocation: Option<WorkbenchForkBudget>,
    ) -> Result<WorkbenchForkRequest, AppProtocolError> {
        WorkbenchForkRequest::new(
            WorkbenchQuery::new(
                ConversationId::new([1; 16]).expect("conversation"),
                WorkspaceId::new([2; 16]).expect("workspace"),
            ),
            ConversationTitle::new("Fork".to_owned()).expect("title"),
            ControlOperationId::new([3; 16]).expect("checkpoint"),
            1,
            1,
            1,
            1,
            mode,
            allocation,
        )
    }

    #[test]
    fn read_only_allocation_is_optional_but_isolated_allocation_is_required() {
        let allocation = WorkbenchForkBudget::new(1, 1, 1, 1).expect("allocation");
        assert!(request(WorkbenchForkMode::ReadOnlyCurrentWorkspace, None).is_ok());
        assert!(request(WorkbenchForkMode::ReadOnlyCurrentWorkspace, Some(allocation)).is_ok());
        assert!(request(WorkbenchForkMode::IsolatedWritableWorkspace, None).is_err());
        assert!(request(WorkbenchForkMode::IsolatedWritableWorkspace, Some(allocation)).is_ok());
    }
}
