//! Product-level coding-run messages exposed to interactive clients.

mod binding;
mod artifact;
mod control;
mod effort;
mod error;
mod interaction;
pub use binding::ProductInteractionBinding;
pub use artifact::*;
mod models;
mod observation;
mod operation;
mod page;
mod phase;
mod query;
mod request;
mod settlement;
mod snapshot;

pub use control::*;
pub use effort::ProductModelEffort;
pub use error::ProductRunMessageError;
pub use interaction::*;
pub use models::*;
pub use observation::ProductRunObservation;
pub use operation::*;
pub use page::*;
pub use phase::*;
pub use query::*;
pub use request::*;
pub use settlement::*;
pub use snapshot::*;

use std::path::{Component, Path};

use peritus_run_settlement::CandidateStage;
use peritus_types::RunId;

/// Maximum UTF-8 bytes accepted for one coding task.
pub const MAX_PRODUCT_TASK_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 bytes retained in one user-facing run field.
pub const MAX_PRODUCT_DETAIL_BYTES: usize = 1024 * 1024;
/// Maximum product runs returned by one page operation.
pub const MAX_PRODUCT_RUN_PAGE: usize = 256;
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
    fn checked(
        workspace_path: String,
        changed_paths: Vec<String>,
        successful_commands: Vec<String>,
        run_instructions: String,
        qualification: CandidateStage,
        require_successful_command: bool,
    ) -> Result<Self, ProductRunMessageError> {
        if workspace_path.trim().is_empty() || run_instructions.trim().is_empty() {
            return Err(ProductRunMessageError::Empty);
        }
        ProductArtifactReference::measure(&workspace_path)?;
        ProductArtifactReference::measure(&run_instructions)?;
        if changed_paths.is_empty()
            || require_successful_command && successful_commands.is_empty()
        {
            return Err(ProductRunMessageError::TooManyDeliverableItems);
        }
        for value in changed_paths.iter().chain(&successful_commands) {
            if value.trim().is_empty() {
                return Err(ProductRunMessageError::Empty);
            }
            ProductArtifactReference::measure(value)?;
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

    /// Exact content-free handoff references used by bounded product-run projections.
    ///
    /// # Errors
    /// Rejects an internally inconsistent retained deliverable.
    pub fn reference(&self) -> Result<ProductDeliverableReference, ProductRunMessageError> {
        ProductDeliverableReference::new(
            ProductArtifactReference::measure(&self.workspace_path)?,
            product_deliverable_index_reference(
                ProductDeliverableIndexKind::ChangedPaths,
                &self.changed_paths,
            )?,
            product_deliverable_index_reference(
                ProductDeliverableIndexKind::SuccessfulCommands,
                &self.successful_commands,
            )?,
            ProductArtifactReference::measure(&self.run_instructions)?,
            self.qualification,
            self.accepted,
            (!self.commit_revision.is_empty())
                .then(|| ProductArtifactReference::measure(&self.commit_revision))
                .transpose()?,
            (!self.export_path.is_empty())
                .then(|| ProductArtifactReference::measure(&self.export_path))
                .transpose()?,
            self.discarded,
        )
    }

    /// Strongest automated qualification stage for the exact candidate.
    #[must_use]
    pub const fn qualification(&self) -> CandidateStage {
        self.qualification
    }

    /// Creates a bounded handoff at an explicit automated qualification stage.
    ///
    /// # Errors
    ///
    /// Rejects missing paths or instructions, empty candidate paths, invalid qualification, or
    /// oversized collections.
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
        if revision.trim().is_empty() {
            return Err(ProductRunMessageError::Empty);
        }
        ProductArtifactReference::measure(&revision)?;
        self.commit_revision = revision;
        self.accepted = true;
        Ok(self)
    }

    /// Returns a handoff carrying the exported patch path.
    ///
    /// # Errors
    /// Rejects an empty or oversized export path.
    pub fn mark_exported(mut self, path: String) -> Result<Self, ProductRunMessageError> {
        if path.trim().is_empty() {
            return Err(ProductRunMessageError::Empty);
        }
        ProductArtifactReference::measure(&path)?;
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
        if !commit_revision.is_empty() {
            if commit_revision.trim().is_empty() {
                return Err(ProductRunMessageError::Empty);
            }
            ProductArtifactReference::measure(&commit_revision)?;
        }
        if !export_path.is_empty() {
            if export_path.trim().is_empty() {
                return Err(ProductRunMessageError::Empty);
            }
            ProductArtifactReference::measure(&export_path)?;
        }
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

/// Query for one page of recent runs or one exact run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunQuery {
    run_id: Option<RunId>,
    offset: u64,
}

impl ProductRunQuery {
    /// Queries the bounded recent-run list.
    #[must_use]
    pub const fn recent() -> Self {
        Self::page(0)
    }
    /// Queries one bounded recent-run page at the zero-based offset.
    #[must_use]
    pub const fn page(offset: u64) -> Self {
        Self { run_id: None, offset }
    }
    /// Queries one exact run.
    #[must_use]
    pub const fn exact(run_id: RunId) -> Self {
        Self { run_id: Some(run_id), offset: 0 }
    }
    /// Optional exact run filter.
    #[must_use]
    pub const fn run_id(self) -> Option<RunId> {
        self.run_id
    }
    /// Zero-based offset for a recent-run page.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
}
