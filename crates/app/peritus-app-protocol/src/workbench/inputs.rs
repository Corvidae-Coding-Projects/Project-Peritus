//! Bounded queue commands and revision-fenced paginated projections.

use crate::{AppErrorCode, AppProtocolError, WorkbenchInputId, WorkbenchQuery};

/// Maximum inputs in one queue page; full history is paginated, never silently truncated.
pub const MAX_WORKBENCH_INPUT_PAGE: usize = 32;
/// Maximum queued identities in a reorder operation.
pub const MAX_WORKBENCH_PENDING_INPUTS: usize = 1024;
/// Maximum declared prerequisites for one input.
pub const MAX_WORKBENCH_INPUT_DEPENDENCIES: usize = 32;
/// Maximum exact text bytes in a single user input revision.
pub const MAX_WORKBENCH_INPUT_BYTES: usize = 8192;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Exact inert user-input text; newline and tab are allowed, terminal escape controls are not.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchInputText(String);
impl WorkbenchInputText {
    /// Validates exact text without interpreting slash commands or markup.
    ///
    /// # Errors
    /// Rejects empty, oversized or terminal-control-bearing text.
    pub fn new(text: String) -> Result<Self, AppProtocolError> {
        if text.trim().is_empty()
            || text.len() > MAX_WORKBENCH_INPUT_BYTES
            || text.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(invalid());
        }
        Ok(Self(text))
    }
    /// Borrows exact accepted text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for WorkbenchInputText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkbenchInputText").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}

/// Exact immutable content revision selected by the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchInputSelection {
    id: WorkbenchInputId,
    revision: u64,
}
impl WorkbenchInputSelection {
    /// Validates a positive content revision.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub const fn new(id: WorkbenchInputId, revision: u64) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(invalid());
        }
        Ok(Self { id, revision })
    }
    /// Returns stable input identity.
    #[must_use]
    pub const fn id(self) -> WorkbenchInputId {
        self.id
    }
    /// Returns exact selected content revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// An ordered duplicate-free bounded identity list; the host checks dependency ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchInputOrder(Vec<WorkbenchInputId>);
impl WorkbenchInputOrder {
    /// Validates the maximum pending set and uniqueness without sorting away user order.
    ///
    /// # Errors
    /// Rejects duplicates or an oversized list.
    pub fn new(ids: Vec<WorkbenchInputId>) -> Result<Self, AppProtocolError> {
        if ids.len() > MAX_WORKBENCH_PENDING_INPUTS
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        {
            return Err(invalid());
        }
        Ok(Self(ids))
    }
    /// Borrows exact requested order.
    #[must_use]
    pub fn ids(&self) -> &[WorkbenchInputId] {
        &self.0
    }
}

/// Exact new input and declared prerequisites. Neither presence nor acceptance starts execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchNewInput {
    id: WorkbenchInputId,
    text: WorkbenchInputText,
    dependencies: WorkbenchInputOrder,
}
impl WorkbenchNewInput {
    /// Checks the smaller prerequisite bound and prevents a direct self-dependency.
    ///
    /// # Errors
    /// Rejects excess or self-referential prerequisites.
    pub fn new(
        id: WorkbenchInputId,
        text: WorkbenchInputText,
        dependencies: WorkbenchInputOrder,
    ) -> Result<Self, AppProtocolError> {
        if dependencies.ids().len() > MAX_WORKBENCH_INPUT_DEPENDENCIES
            || dependencies.ids().contains(&id)
        {
            return Err(invalid());
        }
        Ok(Self { id, text, dependencies })
    }
    /// Returns new input identity.
    #[must_use]
    pub const fn id(&self) -> WorkbenchInputId {
        self.id
    }
    /// Borrows exact text.
    #[must_use]
    pub const fn text(&self) -> &WorkbenchInputText {
        &self.text
    }
    /// Borrows prerequisite identities.
    #[must_use]
    pub const fn dependencies(&self) -> &WorkbenchInputOrder {
        &self.dependencies
    }
}

/// Closed user queue operations. Incorporation is deliberately absent: only the host can bind it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkbenchQueueIntent {
    /// Accepts a new pending input with exact author-bound receipt.
    Enqueue(WorkbenchNewInput),
    /// Supersedes only an unincorporated revision.
    Edit {
        /// Exact inspected revision.
        selected: WorkbenchInputSelection,
        /// Replacement text.
        text: WorkbenchInputText,
    },
    /// Adds a new correction without changing already-incorporated history.
    Correct {
        /// Original incorporated input.
        original: WorkbenchInputSelection,
        /// New correction identity.
        id: WorkbenchInputId,
        /// Corrective text.
        text: WorkbenchInputText,
    },
    /// Holds or releases an unincorporated revision.
    Hold {
        /// Exact inspected revision.
        selected: WorkbenchInputSelection,
        /// Desired hold state.
        held: bool,
    },
    /// Withdraws an exact unincorporated input, retaining immutable history.
    Withdraw(WorkbenchInputSelection),
    /// Reorders the complete pending set subject to dependency checks.
    Reorder(WorkbenchInputOrder),
}

/// Public lifecycle projection of one immutable content revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchInputState {
    /// Eligible when prerequisites are available.
    Queued,
    /// Explicitly excluded until released.
    Held,
    /// Exact input is bound to a constructed model request.
    Incorporated,
    /// A newer unincorporated content revision replaced this one.
    Superseded,
    /// Explicitly removed from eligibility.
    Withdrawn,
}

/// One bounded revisioned history row, not an execution acknowledgement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchInputRow {
    selected: WorkbenchInputSelection,
    text: WorkbenchInputText,
    state: WorkbenchInputState,
    dependencies: WorkbenchInputOrder,
}
impl WorkbenchInputRow {
    /// Creates a public projection of an already checked durable row.
    ///
    /// # Errors
    /// Rejects excess dependencies or direct self-dependency.
    pub fn new(
        selected: WorkbenchInputSelection,
        text: WorkbenchInputText,
        state: WorkbenchInputState,
        dependencies: WorkbenchInputOrder,
    ) -> Result<Self, AppProtocolError> {
        let value = WorkbenchNewInput::new(selected.id(), text, dependencies)?;
        Ok(Self { selected, text: value.text, state, dependencies: value.dependencies })
    }
    /// Returns the exact input revision.
    #[must_use]
    pub const fn selected(&self) -> WorkbenchInputSelection {
        self.selected
    }
    /// Borrows exact immutable text.
    #[must_use]
    pub const fn text(&self) -> &WorkbenchInputText {
        &self.text
    }
    /// Returns observed lifecycle.
    #[must_use]
    pub const fn state(&self) -> WorkbenchInputState {
        self.state
    }
    /// Borrows declared prerequisites.
    #[must_use]
    pub const fn dependencies(&self) -> &WorkbenchInputOrder {
        &self.dependencies
    }
}

/// Read-only paginated input query; subsequent pages must bind the first page's revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchQueueQuery {
    query: WorkbenchQuery,
    revision: u64,
    offset: u32,
    history: bool,
}
impl WorkbenchQueueQuery {
    /// Validates a revision fence for noninitial pages. Zero revision selects current page zero.
    ///
    /// # Errors
    /// Rejects an unfenced noninitial page or an out-of-bound offset.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        offset: u32,
        history: bool,
    ) -> Result<Self, AppProtocolError> {
        if (offset != 0 && revision == 0) || offset as usize > MAX_WORKBENCH_PENDING_INPUTS {
            return Err(invalid());
        }
        Ok(Self { query, revision, offset, history })
    }
    /// Returns exact conversation/workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns exact snapshot fence or zero for the current first page.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns first requested row offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
    /// Returns whether the view includes historical revisions rather than only pending order.
    #[must_use]
    pub const fn history(self) -> bool {
        self.history
    }
}

/// Bounded exact queue page, with total count so partial data cannot be mistaken for full history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchQueuePage {
    query: WorkbenchQueueQuery,
    total: u32,
    rows: Vec<WorkbenchInputRow>,
}
impl WorkbenchQueuePage {
    /// Validates committed revision and exact page coverage.
    ///
    /// # Errors
    /// Rejects gaps, overlarge pages, duplicate revisions or absent-state snapshots.
    pub fn new(
        query: WorkbenchQueueQuery,
        total: u32,
        rows: Vec<WorkbenchInputRow>,
    ) -> Result<Self, AppProtocolError> {
        let expected =
            (total.saturating_sub(query.offset()) as usize).min(MAX_WORKBENCH_INPUT_PAGE);
        if query.revision() == 0
            || query.offset() > total
            || total as usize > MAX_WORKBENCH_PENDING_INPUTS
            || rows.len() != expected
            || rows.iter().enumerate().any(|(index, row)| {
                rows[..index].iter().any(|prior| prior.selected() == row.selected())
            })
        {
            return Err(invalid());
        }
        Ok(Self { query, total, rows })
    }
    /// Returns exact scope, revision and view parameters.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQueueQuery {
        self.query
    }
    /// Returns full row count in this revision-fenced view.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }
    /// Borrows this page's exact rows.
    #[must_use]
    pub fn rows(&self) -> &[WorkbenchInputRow] {
        &self.rows
    }
}
