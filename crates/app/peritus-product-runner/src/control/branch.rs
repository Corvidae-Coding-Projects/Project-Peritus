//! Durable conversation-branch lineage and governing child-budget allocation.

use super::{ControlError, ControlText, ConversationId, GoalBudget, GoalCriterion, OperationId};
use peritus_types::WorkspaceId;
use serde::Deserialize;
use serde::Serialize;

/// Filesystem relationship selected for a newly forked conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationBranchMode {
    /// Shares only the current workspace view and cannot admit writes.
    ReadOnlyCurrentWorkspace,
    /// Uses a separately registered workspace with independent write ownership.
    IsolatedWritableWorkspace,
}

/// Exact budget slice reserved from the source goal for one governed child.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildBudgetAllocation {
    active_millis: u64,
    requests: u32,
    tool_calls: u32,
    total_tokens: u64,
}

/// Budget capacity durably removed from a source goal and assigned to children.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildBudgetReservation {
    active_millis: u64,
    requests: u32,
    tool_calls: u32,
    total_tokens: u64,
}

impl ChildBudgetReservation {
    /// Returns reserved active execution time.
    #[must_use]
    pub const fn active_millis(self) -> u64 {
        self.active_millis
    }
    /// Returns reserved provider requests.
    #[must_use]
    pub const fn requests(self) -> u32 {
        self.requests
    }
    /// Returns reserved tool calls.
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Returns reserved reported/derived tokens.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }

    pub(super) fn checked_add(
        self,
        allocation: ChildBudgetAllocation,
    ) -> Result<Self, ControlError> {
        Ok(Self {
            active_millis: self
                .active_millis
                .checked_add(allocation.active_millis)
                .ok_or(ControlError::Capacity)?,
            requests: self
                .requests
                .checked_add(allocation.requests)
                .ok_or(ControlError::Capacity)?,
            tool_calls: self
                .tool_calls
                .checked_add(allocation.tool_calls)
                .ok_or(ControlError::Capacity)?,
            total_tokens: self
                .total_tokens
                .checked_add(allocation.total_tokens)
                .ok_or(ControlError::Capacity)?,
        })
    }
}

impl ChildBudgetAllocation {
    /// Creates a positive fully bounded child allocation under installed hard ceilings.
    ///
    /// # Errors
    /// Rejects zero fields or values outside the existing goal budget contract.
    pub fn new(
        active_millis: u64,
        requests: u32,
        tool_calls: u32,
        total_tokens: u64,
    ) -> Result<Self, ControlError> {
        GoalBudget::new(Some(active_millis), Some(requests), Some(tool_calls), Some(total_tokens))?;
        Ok(Self { active_millis, requests, tool_calls, total_tokens })
    }

    /// Returns the active-execution allocation.
    #[must_use]
    pub const fn active_millis(self) -> u64 {
        self.active_millis
    }
    /// Returns the provider-request allocation.
    #[must_use]
    pub const fn requests(self) -> u32 {
        self.requests
    }
    /// Returns the tool-call allocation.
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Returns the reported/derived token allocation.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    /// Converts the reserved slice into the child's ordinary cumulative goal limits.
    ///
    /// # Panics
    /// Cannot panic for a value produced by [`Self::new`]; it repeats the same checked limits.
    #[must_use]
    pub fn goal_budget(self) -> GoalBudget {
        GoalBudget::new(
            Some(self.active_millis),
            Some(self.requests),
            Some(self.tool_calls),
            Some(self.total_tokens),
        )
        .expect("validated child allocation remains a valid goal budget")
    }
}

/// Exact immutable source/checkpoint lineage and safe initial child state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationBranch {
    operation: OperationId,
    source: ConversationId,
    source_workspace: [u8; 16],
    source_revision: u64,
    checkpoint: OperationId,
    context_generation: u64,
    brief_revision: u64,
    goal_revision: u64,
    child: ConversationId,
    child_workspace: [u8; 16],
    mode: ConversationBranchMode,
    title: ControlText<256>,
    objective: Option<ControlText<8192>>,
    criteria: Vec<GoalCriterion>,
    allocation: Option<ChildBudgetAllocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seed: Option<Box<super::ConversationSeed>>,
}

impl ConversationBranch {
    /// Constructs a branch which carries public source context but no executable authority state.
    ///
    /// # Errors
    /// Rejects inconsistent workspace/mode bindings, absent revisions, or invalid draft state.
    #[allow(
        clippy::too_many_arguments,
        reason = "fork lineage fields remain explicit and independently checked"
    )]
    pub fn new(
        operation: OperationId,
        source: ConversationId,
        source_workspace: WorkspaceId,
        source_revision: u64,
        checkpoint: OperationId,
        context_generation: u64,
        brief_revision: u64,
        goal_revision: u64,
        child: ConversationId,
        child_workspace: WorkspaceId,
        mode: ConversationBranchMode,
        title: String,
        objective: Option<String>,
        criteria: Vec<GoalCriterion>,
        allocation: Option<ChildBudgetAllocation>,
    ) -> Result<Self, ControlError> {
        let value = Self {
            operation,
            source,
            source_workspace: source_workspace.into_bytes(),
            source_revision,
            checkpoint,
            context_generation,
            brief_revision,
            goal_revision,
            child,
            child_workspace: child_workspace.into_bytes(),
            mode,
            title: ControlText::new(title)?,
            objective: objective.map(ControlText::new).transpose()?,
            criteria,
            allocation,
            seed: None,
        };
        value.validate()?;
        Ok(value)
    }

    /// Attaches context from the exact historical source, without carrying execution authority.
    ///
    /// # Errors
    /// Rejects a source identity or revision mismatch.
    pub fn with_seed(mut self, seed: super::ConversationSeed) -> Result<Self, ControlError> {
        if !seed.matches(self.source, self.source_revision) {
            return Err(ControlError::ScopeMismatch);
        }
        self.seed = Some(Box::new(seed));
        Ok(self)
    }

    /// Borrows the immutable historical context, if present in this branch version.
    #[must_use]
    pub fn seed(&self) -> Option<&super::ConversationSeed> {
        self.seed.as_deref()
    }

    /// Returns the user operation identifying this fork.
    #[must_use]
    pub const fn operation(&self) -> OperationId {
        self.operation
    }
    /// Returns the exact source conversation.
    #[must_use]
    pub const fn source(&self) -> ConversationId {
        self.source
    }
    /// Returns the source workspace bytes.
    #[must_use]
    pub const fn source_workspace_bytes(&self) -> &[u8; 16] {
        &self.source_workspace
    }
    /// Returns the exact selected source revision.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    /// Returns the selected checkpoint identity.
    #[must_use]
    pub const fn checkpoint(&self) -> OperationId {
        self.checkpoint
    }
    /// Returns the source context generation.
    #[must_use]
    pub const fn context_generation(&self) -> u64 {
        self.context_generation
    }
    /// Returns the source brief revision, or zero when absent.
    #[must_use]
    pub const fn brief_revision(&self) -> u64 {
        self.brief_revision
    }
    /// Returns the source goal revision, or zero when absent.
    #[must_use]
    pub const fn goal_revision(&self) -> u64 {
        self.goal_revision
    }
    /// Returns the independent child conversation.
    #[must_use]
    pub const fn child(&self) -> ConversationId {
        self.child
    }
    /// Returns the child workspace bytes.
    #[must_use]
    pub const fn child_workspace_bytes(&self) -> &[u8; 16] {
        &self.child_workspace
    }
    /// Returns the selected branch workspace mode.
    #[must_use]
    pub const fn mode(&self) -> ConversationBranchMode {
        self.mode
    }
    /// Borrows the child title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.title.as_str()
    }
    /// Borrows the inherited objective used only to seed a draft.
    #[must_use]
    pub fn objective(&self) -> Option<&str> {
        self.objective.as_ref().map(ControlText::as_str)
    }
    /// Borrows fresh, unsatisfied child criteria.
    #[must_use]
    pub fn criteria(&self) -> &[GoalCriterion] {
        &self.criteria
    }
    /// Returns the reserved governing budget slice.
    #[must_use]
    pub const fn allocation(&self) -> Option<ChildBudgetAllocation> {
        self.allocation
    }

    /// Encodes the bounded immutable lineage projection for journal publication.
    ///
    /// # Errors
    /// Rejects invalid state or an oversized encoding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ControlError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ControlError::InvalidInput)?;
        if bytes.len() > super::MAX_CONTROL_BYTES { Err(ControlError::Capacity) } else { Ok(bytes) }
    }

    /// Decodes and validates an exact lineage projection.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, or inconsistent state.
    pub fn parse(bytes: &[u8]) -> Result<Self, ControlError> {
        if bytes.len() > super::MAX_CONTROL_BYTES {
            return Err(ControlError::Capacity);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| ControlError::InvalidInput)?;
        value.validate()?;
        if value.canonical_bytes()? != bytes {
            return Err(ControlError::InvalidInput);
        }
        Ok(value)
    }

    fn validate(&self) -> Result<(), ControlError> {
        if self.seed.as_ref().is_some_and(|seed| !seed.matches(self.source, self.source_revision)) {
            return Err(ControlError::ScopeMismatch);
        }
        if self.source_revision == 0 || self.source == self.child {
            return Err(ControlError::InvalidInput);
        }
        WorkspaceId::new(self.source_workspace).map_err(|_| ControlError::InvalidInput)?;
        WorkspaceId::new(self.child_workspace).map_err(|_| ControlError::InvalidInput)?;
        match self.mode {
            ConversationBranchMode::ReadOnlyCurrentWorkspace
                if self.source_workspace == self.child_workspace => {}
            ConversationBranchMode::IsolatedWritableWorkspace
                if self.source_workspace != self.child_workspace
                    && self.allocation.is_some()
                    && self.objective.is_some()
                    && !self.criteria.is_empty() => {}
            _ => return Err(ControlError::InvalidInput),
        }
        if self.criteria.len() > 16 {
            return Err(ControlError::Capacity);
        }
        Ok(())
    }
}
