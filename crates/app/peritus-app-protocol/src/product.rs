//! Product-level coding-run messages exposed to interactive clients.

mod control;
mod conversation;
mod phase;
mod request;

pub use control::*;
pub use conversation::*;
pub use phase::*;
pub use request::*;

use core::fmt;
use std::path::{Component, Path};

use peritus_types::{RunId, WorkspaceId};

/// Maximum UTF-8 bytes accepted for one coding task.
pub const MAX_PRODUCT_TASK_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 bytes retained in one user-facing run field.
pub const MAX_PRODUCT_DETAIL_BYTES: usize = 1024 * 1024;
/// Maximum product runs returned by one list operation.
pub const MAX_PRODUCT_RUNS: usize = 256;
/// Maximum exact changed paths retained in a completion handoff.
pub const MAX_PRODUCT_DELIVERABLE_PATHS: usize = 512;
/// Maximum exact successful commands retained in a completion handoff.
pub const MAX_PRODUCT_DELIVERABLE_COMMANDS: usize = 256;

/// Durable user-facing handoff for one accepted E0 candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductDeliverable {
    workspace_path: String,
    changed_paths: Vec<String>,
    successful_commands: Vec<String>,
    run_instructions: String,
    accepted: bool,
    commit_revision: String,
    export_path: String,
    discarded: bool,
}

impl ProductDeliverable {
    /// Creates a bounded pending handoff.
    ///
    /// # Errors
    /// Rejects missing paths/instructions, empty candidate paths, or oversized collections.
    pub fn new(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&workspace_path, MAX_PRODUCT_DETAIL_BYTES)?;
        bounded_text(&run_instructions, MAX_PRODUCT_DETAIL_BYTES)?;
        if changed_paths.is_empty()
            || changed_paths.len() > MAX_PRODUCT_DELIVERABLE_PATHS
            || successful_commands.is_empty()
            || successful_commands.len() > MAX_PRODUCT_DELIVERABLE_COMMANDS
        {
            return Err(ProductRunMessageError::TooManyDeliverableItems);
        }
        for value in changed_paths.iter().chain(&successful_commands) {
            bounded_text(value, MAX_PRODUCT_DETAIL_BYTES)?;
        }
        if changed_paths.iter().any(|value| {
            let path = Path::new(value);
            path.is_absolute()
                || path.components().any(|component| !matches!(component, Component::Normal(_)))
                || path.starts_with(".git")
        }) {
            return Err(ProductRunMessageError::InvalidDeliverablePath);
        }
        Ok(Self {
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            accepted: false,
            commit_revision: String::new(),
            export_path: String::new(),
            discarded: false,
        })
    }

    /// Managed worktree containing the deliverable.
    #[must_use]
    pub fn workspace_path(&self) -> &str {
        &self.workspace_path
    }

    /// Exact task candidate paths.
    #[must_use]
    pub fn changed_paths(&self) -> &[String] {
        &self.changed_paths
    }

    /// Exact acceptance commands that exited successfully.
    #[must_use]
    pub fn successful_commands(&self) -> &[String] {
        &self.successful_commands
    }

    /// Concrete command or steps for running the result.
    #[must_use]
    pub fn run_instructions(&self) -> &str {
        &self.run_instructions
    }

    /// Whether the user explicitly accepted the handoff.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        self.accepted
    }

    /// Managed Git commit created for this deliverable, when requested.
    #[must_use]
    pub fn commit_revision(&self) -> &str {
        &self.commit_revision
    }

    /// Exported patch path, when requested.
    #[must_use]
    pub fn export_path(&self) -> &str {
        &self.export_path
    }

    /// Whether the exact deliverable was discarded.
    #[must_use]
    pub const fn discarded(&self) -> bool {
        self.discarded
    }

    /// Returns an accepted handoff.
    #[must_use]
    pub const fn mark_accepted(mut self) -> Self {
        self.accepted = true;
        self
    }

    /// Returns a handoff carrying the created commit revision.
    ///
    /// # Errors
    /// Rejects an empty or oversized revision string.
    pub fn mark_committed(mut self, revision: String) -> Result<Self, ProductRunMessageError> {
        bounded_text(&revision, MAX_PRODUCT_DETAIL_BYTES)?;
        self.commit_revision = revision;
        self.accepted = true;
        Ok(self)
    }

    /// Returns a handoff carrying the exported patch path.
    ///
    /// # Errors
    /// Rejects an empty or oversized export path.
    pub fn mark_exported(mut self, path: String) -> Result<Self, ProductRunMessageError> {
        bounded_text(&path, MAX_PRODUCT_DETAIL_BYTES)?;
        self.export_path = path;
        Ok(self)
    }

    /// Returns a discarded handoff.
    #[must_use]
    pub const fn mark_discarded(mut self) -> Self {
        self.discarded = true;
        self
    }

    #[allow(clippy::too_many_arguments, reason = "wire restoration keeps durable fields explicit")]
    pub(crate) fn restore(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
        accepted: bool,
        commit_revision: String,
        export_path: String,
        discarded: bool,
    ) -> Result<Self, ProductRunMessageError> {
        let mut value =
            Self::new(workspace_path, changed_paths, successful_commands, run_instructions)?;
        optional_bounded_text(&commit_revision, MAX_PRODUCT_DETAIL_BYTES)?;
        optional_bounded_text(&export_path, MAX_PRODUCT_DETAIL_BYTES)?;
        value.accepted = accepted;
        value.commit_revision = commit_revision;
        value.export_path = export_path;
        value.discarded = discarded;
        Ok(value)
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
    deliverable: Option<ProductDeliverable>,
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
            deliverable: None,
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

    /// Durable deliverable handoff after exact E0 acceptance.
    #[must_use]
    pub const fn deliverable(&self) -> Option<&ProductDeliverable> {
        self.deliverable.as_ref()
    }

    /// Attaches or replaces a checked deliverable handoff.
    #[must_use]
    pub fn with_deliverable(mut self, deliverable: ProductDeliverable) -> Self {
        self.deliverable = Some(deliverable);
        self
    }
}

/// Failure to construct a bounded product-run message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunMessageError {
    /// A required string is empty or whitespace-only.
    Empty,
    /// A string exceeds its compiled protocol bound.
    TooLong,
    /// A conversation exceeds its retained message limit.
    TooManyMessages,
    /// A deliverable contains too many paths or commands, or lacks either collection.
    TooManyDeliverableItems,
    /// A deliverable path is absolute, traversing, or targets Git metadata.
    InvalidDeliverablePath,
}

impl fmt::Display for ProductRunMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "product run text is empty",
            Self::TooLong => "product run text exceeds its protocol bound",
            Self::TooManyMessages => "product run conversation has too many messages",
            Self::TooManyDeliverableItems => {
                "product deliverable path or command collection is invalid"
            }
            Self::InvalidDeliverablePath => "product deliverable contains an unsafe path",
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
