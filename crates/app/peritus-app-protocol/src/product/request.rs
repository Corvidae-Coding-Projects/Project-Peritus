//! Checked requests and provider-role selection for product runs.

use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::{MAX_PRODUCT_TASK_BYTES, ProductRunMessageError, bounded_text};

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
