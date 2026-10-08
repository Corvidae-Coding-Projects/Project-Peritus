//! Daemon-owned execution request, separate from the public application protocol.

use peritus_app_protocol::{ProductArtifactReference, ProductProviderSelection, ProductRunMessageError};
use peritus_types::{RunId, WorkspaceId};

/// One admitted execution with distinct model input and user-facing identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProductRunRequest {
    run_id: RunId,
    workspace_id: WorkspaceId,
    providers: ProductProviderSelection,
    execution_task: String,
    display_task: String,
}

impl ProductRunRequest {
    pub(super) fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        providers: ProductProviderSelection,
        execution_task: String,
    ) -> Result<Self, ProductRunMessageError> {
        validate(&execution_task)?;
        Ok(Self {
            run_id,
            workspace_id,
            providers,
            display_task: execution_task.clone(),
            execution_task,
        })
    }

    pub(super) fn with_display_task(
        mut self,
        display_task: String,
    ) -> Result<Self, ProductRunMessageError> {
        validate(&display_task)?;
        self.display_task = display_task;
        Ok(self)
    }

    pub(super) const fn run_id(&self) -> RunId {
        self.run_id
    }

    pub(super) const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub(super) const fn providers(&self) -> ProductProviderSelection {
        self.providers
    }

    pub(super) fn execution_task(&self) -> &str {
        &self.execution_task
    }

    pub(super) fn display_task(&self) -> &str {
        &self.display_task
    }
}

fn validate(value: &str) -> Result<(), ProductRunMessageError> {
    if value.trim().is_empty() {
        return Err(ProductRunMessageError::Empty);
    }
    ProductArtifactReference::measure(value).map(|_| ())
}
