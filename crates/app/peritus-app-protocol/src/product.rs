//! Product-level coding-run messages exposed to interactive clients.

mod control;
mod conversation;
mod error;
mod phase;
mod request;
mod settlement;
mod snapshot;

pub use control::*;
pub use conversation::*;
pub use error::ProductRunMessageError;
pub use phase::*;
pub use request::*;
pub use settlement::*;
pub use snapshot::*;

use std::path::{Component, Path};

use peritus_run_settlement::CandidateStage;
use peritus_types::RunId;

use error::{bounded_text, optional_bounded_text};

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

/// Durable user-facing handoff for one exact E0 candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductDeliverable {
    workspace_path: String,
    changed_paths: Vec<String>,
    successful_commands: Vec<String>,
    run_instructions: String,
    qualification: CandidateStage,
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
        Self::checked(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            CandidateStage::Qualified,
            true,
        )
    }

    fn checked(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
        qualification: CandidateStage,
        require_successful_command: bool,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&workspace_path, MAX_PRODUCT_DETAIL_BYTES)?;
        bounded_text(&run_instructions, MAX_PRODUCT_DETAIL_BYTES)?;
        if changed_paths.is_empty()
            || changed_paths.len() > MAX_PRODUCT_DELIVERABLE_PATHS
            || require_successful_command && successful_commands.is_empty()
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
            qualification,
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

    /// Strongest automated qualification stage for the exact candidate.
    #[must_use]
    pub const fn qualification(&self) -> CandidateStage {
        self.qualification
    }

    /// Creates a bounded handoff at an explicit automated qualification stage.
    ///
    /// This constructor is used by the settlement protocol. [`Self::new`] remains the legacy
    /// accepted-E0 constructor and therefore defaults to [`CandidateStage::Qualified`].
    ///
    /// # Errors
    ///
    /// Applies the same path, command, and text validation as [`Self::new`].
    pub fn candidate(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
        qualification: CandidateStage,
    ) -> Result<Self, ProductRunMessageError> {
        Self::checked(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            qualification,
            false,
        )
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
        Self::restore_inner(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            accepted,
            commit_revision,
            export_path,
            discarded,
            true,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "wire restoration keeps durable fields explicit")]
    pub(crate) fn restore_candidate(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
        accepted: bool,
        commit_revision: String,
        export_path: String,
        discarded: bool,
    ) -> Result<Self, ProductRunMessageError> {
        Self::restore_inner(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            accepted,
            commit_revision,
            export_path,
            discarded,
            false,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "wire restoration keeps durable fields explicit")]
    fn restore_inner(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
        accepted: bool,
        commit_revision: String,
        export_path: String,
        discarded: bool,
        require_successful_command: bool,
    ) -> Result<Self, ProductRunMessageError> {
        let mut value = Self::checked(
            workspace_path,
            changed_paths,
            successful_commands,
            run_instructions,
            CandidateStage::Qualified,
            require_successful_command,
        )?;
        optional_bounded_text(&commit_revision, MAX_PRODUCT_DETAIL_BYTES)?;
        optional_bounded_text(&export_path, MAX_PRODUCT_DETAIL_BYTES)?;
        value.accepted = accepted;
        value.commit_revision = commit_revision;
        value.export_path = export_path;
        value.discarded = discarded;
        Ok(value)
    }

    pub(crate) const fn restore_qualification(mut self, qualification: CandidateStage) -> Self {
        self.qualification = qualification;
        self
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
