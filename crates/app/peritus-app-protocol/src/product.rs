//! Product-level coding-run messages exposed to interactive clients.

use core::fmt;

use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

/// Maximum UTF-8 bytes accepted for one coding task.
pub const MAX_PRODUCT_TASK_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 bytes retained in one user-facing run field.
pub const MAX_PRODUCT_DETAIL_BYTES: usize = 1024 * 1024;
/// Maximum product runs returned by one list operation.
pub const MAX_PRODUCT_RUNS: usize = 256;

/// User-facing phase of one daemon-owned coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunPhase {
    /// Accepted and waiting for execution.
    Queued,
    /// The writer is preparing and applying an implementation.
    Writing,
    /// Repository gates are running.
    Checking,
    /// An independent reviewer is inspecting the result.
    Reviewing,
    /// The fixer is addressing test or review findings.
    Fixing,
    /// Final gates and review are running.
    Verifying,
    /// The run completed with passing gates and no blocking findings.
    Complete,
    /// The run stopped with an actionable failure.
    Failed,
    /// The user cancelled the run.
    Cancelled,
    /// A daemon restart interrupted work that may be retried.
    RecoveryRequired,
}

impl ProductRunPhase {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Queued => 1,
            Self::Writing => 2,
            Self::Checking => 3,
            Self::Reviewing => 4,
            Self::Fixing => 5,
            Self::Verifying => 6,
            Self::Complete => 7,
            Self::Failed => 8,
            Self::Cancelled => 9,
            Self::RecoveryRequired => 10,
        }
    }

    /// Decodes a stable wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Queued),
            2 => Some(Self::Writing),
            3 => Some(Self::Checking),
            4 => Some(Self::Reviewing),
            5 => Some(Self::Fixing),
            6 => Some(Self::Verifying),
            7 => Some(Self::Complete),
            8 => Some(Self::Failed),
            9 => Some(Self::Cancelled),
            10 => Some(Self::RecoveryRequired),
            _ => None,
        }
    }

    /// Returns whether no more work can run without an explicit retry.
    #[must_use]
    pub const fn terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled | Self::RecoveryRequired)
    }

    /// Returns whether the original request may be started again.
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled | Self::RecoveryRequired)
    }
}

/// Checked provider roles selected for one writer-reviewer-fixer loop.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductProviderSelection {
    writer: ProviderProfileId,
    reviewer: ProviderProfileId,
    fixer: ProviderProfileId,
}

impl ProductProviderSelection {
    /// Creates an explicit role selection. Reusing a provider is permitted.
    #[must_use]
    pub const fn new(
        writer: ProviderProfileId,
        reviewer: ProviderProfileId,
        fixer: ProviderProfileId,
    ) -> Self {
        Self { writer, reviewer, fixer }
    }

    /// Writer profile identity.
    #[must_use]
    pub const fn writer(self) -> ProviderProfileId {
        self.writer
    }
    /// Reviewer profile identity.
    #[must_use]
    pub const fn reviewer(self) -> ProviderProfileId {
        self.reviewer
    }
    /// Fixer profile identity.
    #[must_use]
    pub const fn fixer(self) -> ProviderProfileId {
        self.fixer
    }
}

/// Request to begin one natural-language coding run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunRequest {
    run_id: RunId,
    workspace_id: WorkspaceId,
    providers: ProductProviderSelection,
    task: String,
}

impl ProductRunRequest {
    /// Creates a checked run request.
    ///
    /// # Errors
    ///
    /// Rejects an empty, whitespace-only, or oversized task.
    pub fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        providers: ProductProviderSelection,
        task: String,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&task, MAX_PRODUCT_TASK_BYTES)?;
        Ok(Self { run_id, workspace_id, providers, task })
    }

    /// Requested run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Exact managed workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Explicit role providers.
    #[must_use]
    pub const fn providers(&self) -> ProductProviderSelection {
        self.providers
    }
    /// Natural-language task.
    #[must_use]
    pub fn task(&self) -> &str {
        &self.task
    }
}

/// User control operation for one daemon-owned run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunControlAction {
    /// Cancel active provider and repository work.
    Cancel,
    /// Retry a failed, cancelled, or interrupted run from its original request.
    Retry,
}

impl ProductRunControlAction {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Cancel => 1,
            Self::Retry => 2,
        }
    }
    /// Decodes a stable wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Cancel),
            2 => Some(Self::Retry),
            _ => None,
        }
    }
}

/// Control request for one exact coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunControl {
    run_id: RunId,
    action: ProductRunControlAction,
}

impl ProductRunControl {
    /// Creates an exact run control request.
    #[must_use]
    pub const fn new(run_id: RunId, action: ProductRunControlAction) -> Self {
        Self { run_id, action }
    }
    /// Target run.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    /// Requested action.
    #[must_use]
    pub const fn action(self) -> ProductRunControlAction {
        self.action
    }
}

/// Query for the most recent runs or one exact run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunQuery {
    run_id: Option<RunId>,
}

impl ProductRunQuery {
    /// Queries the bounded recent-run list.
    #[must_use]
    pub const fn recent() -> Self {
        Self { run_id: None }
    }
    /// Queries one exact run.
    #[must_use]
    pub const fn exact(run_id: RunId) -> Self {
        Self { run_id: Some(run_id) }
    }
    /// Optional exact run filter.
    #[must_use]
    pub const fn run_id(self) -> Option<RunId> {
        self.run_id
    }
}

/// Complete bounded observation of one product run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunSnapshot {
    run_id: RunId,
    workspace_id: WorkspaceId,
    providers: ProductProviderSelection,
    phase: ProductRunPhase,
    cycle: u32,
    task: String,
    status: String,
    diff: String,
    gates: String,
    review: String,
    summary: String,
}

impl ProductRunSnapshot {
    /// Creates one checked run observation.
    ///
    /// # Errors
    ///
    /// Rejects empty primary fields or any field above its protocol bound.
    #[allow(clippy::too_many_arguments, reason = "snapshot fields are independently rendered")]
    pub fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        providers: ProductProviderSelection,
        phase: ProductRunPhase,
        cycle: u32,
        task: String,
        status: String,
        diff: String,
        gates: String,
        review: String,
        summary: String,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&task, MAX_PRODUCT_TASK_BYTES)?;
        bounded_text(&status, MAX_PRODUCT_DETAIL_BYTES)?;
        for value in [&diff, &gates, &review, &summary] {
            optional_bounded_text(value, MAX_PRODUCT_DETAIL_BYTES)?;
        }
        Ok(Self {
            run_id,
            workspace_id,
            providers,
            phase,
            cycle,
            task,
            status,
            diff,
            gates,
            review,
            summary,
        })
    }

    /// Run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Role provider identities.
    #[must_use]
    pub const fn providers(&self) -> ProductProviderSelection {
        self.providers
    }
    /// Current phase.
    #[must_use]
    pub const fn phase(&self) -> ProductRunPhase {
        self.phase
    }
    /// One-based writer/fixer cycle.
    #[must_use]
    pub const fn cycle(&self) -> u32 {
        self.cycle
    }
    /// Original task.
    #[must_use]
    pub fn task(&self) -> &str {
        &self.task
    }
    /// Current human-readable operation.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }
    /// Current repository diff.
    #[must_use]
    pub fn diff(&self) -> &str {
        &self.diff
    }
    /// Latest gate output.
    #[must_use]
    pub fn gates(&self) -> &str {
        &self.gates
    }
    /// Latest independent review.
    #[must_use]
    pub fn review(&self) -> &str {
        &self.review
    }
    /// Terminal or interim summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// Failure to construct a bounded product-run message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunMessageError {
    /// A required string is empty or whitespace-only.
    Empty,
    /// A string exceeds its compiled protocol bound.
    TooLong,
}

impl fmt::Display for ProductRunMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "product run text is empty",
            Self::TooLong => "product run text exceeds its protocol bound",
        })
    }
}

impl std::error::Error for ProductRunMessageError {}

fn bounded_text(value: &str, maximum: usize) -> Result<(), ProductRunMessageError> {
    if value.trim().is_empty() {
        Err(ProductRunMessageError::Empty)
    } else {
        optional_bounded_text(value, maximum)
    }
}

const fn optional_bounded_text(value: &str, maximum: usize) -> Result<(), ProductRunMessageError> {
    if value.len() > maximum { Err(ProductRunMessageError::TooLong) } else { Ok(()) }
}
