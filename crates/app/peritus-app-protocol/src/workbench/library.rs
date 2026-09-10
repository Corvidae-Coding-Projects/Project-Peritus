//! Bounded conversation-library search and non-running fork contracts.

use crate::{
    AppErrorCode, AppProtocolError, ControlOperationId, ConversationId, ConversationTitle,
    WorkbenchGoalState, WorkbenchQuery,
};
use peritus_types::{RunId, WorkspaceId};

const MAX_QUERY_BYTES: usize = 256;
const MAX_SNIPPET_BYTES: usize = 512;
const MAX_HANDOFF_BYTES: usize = 1024;
/// Maximum conversations returned in one library page.
pub const MAX_CONVERSATION_LIBRARY_PAGE: u16 = 64;

fn checked_text(value: String, maximum: usize, empty: bool) -> Result<String, AppProtocolError> {
    if value.len() > maximum
        || (!empty && value.trim().is_empty())
        || value.chars().any(char::is_control)
    {
        Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None))
    } else {
        Ok(value)
    }
}

/// Inert literal local-search text; it is never sent to a provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationSearchText(String);
impl ConversationSearchText {
    /// Validates bounded nonempty literal text.
    ///
    /// # Errors
    /// Rejects empty, oversized, or control-character-containing search text.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        checked_text(value, MAX_QUERY_BYTES, false).map(Self)
    }
    /// Borrows exact literal query bytes.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One bounded local conversation-library page request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationLibraryQuery {
    workspace: WorkspaceId,
    literal: Option<ConversationSearchText>,
    include_archived: bool,
    offset: u32,
    limit: u16,
}
impl ConversationLibraryQuery {
    /// Creates a bounded literal query with stable offset pagination.
    ///
    /// # Errors
    /// Rejects zero or oversized page limits.
    pub fn new(
        workspace: WorkspaceId,
        literal: Option<ConversationSearchText>,
        include_archived: bool,
        offset: u32,
        limit: u16,
    ) -> Result<Self, AppProtocolError> {
        if limit == 0 || limit > MAX_CONVERSATION_LIBRARY_PAGE {
            return Err(AppProtocolError::new(AppErrorCode::InvalidLimits, None));
        }
        Ok(Self { workspace, literal, include_archived, offset, limit })
    }
    /// Returns the exact workspace scope.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId {
        self.workspace
    }
    /// Borrows optional literal query text.
    #[must_use]
    pub const fn literal(&self) -> Option<&ConversationSearchText> {
        self.literal.as_ref()
    }
    /// Returns whether archived results are eligible.
    #[must_use]
    pub const fn include_archived(&self) -> bool {
        self.include_archived
    }
    /// Returns the stable result offset.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }
    /// Returns the positive bounded page size.
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }
}

mod fork;
pub use fork::*;

/// Exact durable public-message source for one literal snippet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationMessageSource {
    /// User input from the authoritative conversation ledger.
    Input {
        /// Conversation owning the immutable input ledger.
        conversation: ConversationId,
        /// Stable input identity.
        input: crate::WorkbenchInputId,
        /// Exact immutable content revision.
        revision: u64,
    },
    /// Public agent reply artifact.
    Reply {
        /// Conversation owning the public reply reference.
        conversation: ConversationId,
        /// Exact reply-publication operation.
        operation: ControlOperationId,
    },
    /// Durable message in a legacy run record.
    Legacy {
        /// Durable legacy run identity.
        run: RunId,
        /// Zero-based message position in the exact stored transcript.
        index: u32,
    },
}

/// Exact bounded source-linked match returned by local literal search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationSearchSnippet {
    source: ConversationMessageSource,
    text: String,
}
impl ConversationSearchSnippet {
    /// Validates one inert exact substring around a matched public message.
    ///
    /// # Errors
    /// Rejects empty, oversized, or NUL-containing snippet text.
    pub fn new(source: ConversationMessageSource, text: String) -> Result<Self, AppProtocolError> {
        if text.len() > MAX_SNIPPET_BYTES || text.trim().is_empty() || text.contains('\0') {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { source, text })
    }
    /// Returns the exact source reference.
    #[must_use]
    pub const fn source(&self) -> ConversationMessageSource {
        self.source
    }
    /// Borrows exact UTF-8 source bytes (possibly bounded around the match).
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Optional branch lineage shown when opening a child.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBranchLineage {
    parent: WorkbenchQuery,
    checkpoint: ControlOperationId,
    source_revision: u64,
    context_generation: u64,
    brief_revision: u64,
    goal_revision: u64,
    mode: WorkbenchForkMode,
    allocation: Option<WorkbenchForkBudget>,
}
impl WorkbenchBranchLineage {
    /// Constructs a public projection from validated durable lineage.
    #[allow(clippy::too_many_arguments, reason = "checkpoint lineage remains explicit")]
    #[must_use]
    pub const fn new(
        parent: WorkbenchQuery,
        checkpoint: ControlOperationId,
        source_revision: u64,
        context_generation: u64,
        brief_revision: u64,
        goal_revision: u64,
        mode: WorkbenchForkMode,
        allocation: Option<WorkbenchForkBudget>,
    ) -> Self {
        Self {
            parent,
            checkpoint,
            source_revision,
            context_generation,
            brief_revision,
            goal_revision,
            mode,
            allocation,
        }
    }
    /// Returns parent conversation/workspace.
    #[must_use]
    pub const fn parent(&self) -> WorkbenchQuery {
        self.parent
    }
    /// Returns selected checkpoint.
    #[must_use]
    pub const fn checkpoint(&self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns source conversation revision.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    /// Returns source context generation.
    #[must_use]
    pub const fn context_generation(&self) -> u64 {
        self.context_generation
    }
    /// Returns source brief revision.
    #[must_use]
    pub const fn brief_revision(&self) -> u64 {
        self.brief_revision
    }
    /// Returns source goal revision.
    #[must_use]
    pub const fn goal_revision(&self) -> u64 {
        self.goal_revision
    }
    /// Returns branch workspace mode.
    #[must_use]
    pub const fn mode(&self) -> WorkbenchForkMode {
        self.mode
    }
    /// Returns reserved child allocation.
    #[must_use]
    pub const fn allocation(&self) -> Option<WorkbenchForkBudget> {
        self.allocation
    }
}

/// One bounded result in the workspace conversation library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationLibraryItem {
    query: WorkbenchQuery,
    title: ConversationTitle,
    pinned: bool,
    archived: bool,
    activity_revision: u64,
    legacy_run: Option<RunId>,
    goal_state: Option<WorkbenchGoalState>,
    goal_draft: bool,
    handoff: String,
    snippet: Option<ConversationSearchSnippet>,
    branch: Option<WorkbenchBranchLineage>,
}
impl ConversationLibraryItem {
    /// Creates one exact bounded read-only projection.
    ///
    /// # Errors
    /// Rejects an absent activity revision or invalid bounded handoff text.
    #[allow(
        clippy::too_many_arguments,
        reason = "library metadata remains independently inspectable"
    )]
    pub fn new(
        query: WorkbenchQuery,
        title: ConversationTitle,
        pinned: bool,
        archived: bool,
        activity_revision: u64,
        legacy_run: Option<RunId>,
        goal_state: Option<WorkbenchGoalState>,
        goal_draft: bool,
        handoff: String,
        snippet: Option<ConversationSearchSnippet>,
        branch: Option<WorkbenchBranchLineage>,
    ) -> Result<Self, AppProtocolError> {
        if activity_revision == 0 {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self {
            query,
            title,
            pinned,
            archived,
            activity_revision,
            legacy_run,
            goal_state,
            goal_draft,
            handoff: checked_text(handoff, MAX_HANDOFF_BYTES, true)?,
            snippet,
            branch,
        })
    }
    /// Returns stable conversation/workspace identity.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Borrows title.
    #[must_use]
    pub const fn title(&self) -> &ConversationTitle {
        &self.title
    }
    /// Returns pin state.
    #[must_use]
    pub const fn pinned(&self) -> bool {
        self.pinned
    }
    /// Returns archive state.
    #[must_use]
    pub const fn archived(&self) -> bool {
        self.archived
    }
    /// Returns durable activity ordering revision.
    #[must_use]
    pub const fn activity_revision(&self) -> u64 {
        self.activity_revision
    }
    /// Returns legacy run source when this is a stable legacy mapping.
    #[must_use]
    pub const fn legacy_run(&self) -> Option<RunId> {
        self.legacy_run
    }
    /// Returns current governed goal state.
    #[must_use]
    pub const fn goal_state(&self) -> Option<WorkbenchGoalState> {
        self.goal_state
    }
    /// Returns whether a forked goal remains non-running draft state.
    #[must_use]
    pub const fn goal_draft(&self) -> bool {
        self.goal_draft
    }
    /// Borrows last-state handoff.
    #[must_use]
    pub fn handoff(&self) -> &str {
        &self.handoff
    }
    /// Borrows exact source-linked literal snippet.
    #[must_use]
    pub const fn snippet(&self) -> Option<&ConversationSearchSnippet> {
        self.snippet.as_ref()
    }
    /// Borrows source/checkpoint lineage for a branch.
    #[must_use]
    pub const fn branch(&self) -> Option<&WorkbenchBranchLineage> {
        self.branch.as_ref()
    }
}

/// Bounded deterministic conversation library page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationLibraryPage {
    query: ConversationLibraryQuery,
    total: u32,
    next_offset: Option<u32>,
    items: Vec<ConversationLibraryItem>,
}
impl ConversationLibraryPage {
    /// Validates a page against its requested bound and total.
    ///
    /// # Errors
    /// Rejects excess items or an invalid forward pagination offset.
    pub fn new(
        query: ConversationLibraryQuery,
        total: u32,
        next_offset: Option<u32>,
        items: Vec<ConversationLibraryItem>,
    ) -> Result<Self, AppProtocolError> {
        if items.len() > usize::from(query.limit())
            || next_offset.is_some_and(|next| next <= query.offset() || next > total)
        {
            return Err(AppProtocolError::new(AppErrorCode::InvalidLimits, None));
        }
        Ok(Self { query, total, next_offset, items })
    }
    /// Borrows exact query.
    #[must_use]
    pub const fn query(&self) -> &ConversationLibraryQuery {
        &self.query
    }
    /// Returns total matching conversations.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }
    /// Returns next stable offset.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u32> {
        self.next_offset
    }
    /// Borrows ordered results.
    #[must_use]
    pub fn items(&self) -> &[ConversationLibraryItem] {
        &self.items
    }
}
