//! Bounded product-run observation snapshots.

use peritus_types::{RunId, WorkspaceId};

use super::{
    ProductArtifactReference, ProductDeliverable, ProductProviderSelection,
    ProductRunMessageError, ProductRunOperation, ProductRunPhase,
};

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
    operation: ProductRunOperation,
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
        operation: ProductRunOperation,
    ) -> Result<Self, ProductRunMessageError> {
        if task.trim().is_empty() || status.trim().is_empty() {
            return Err(ProductRunMessageError::Empty);
        }
        ProductArtifactReference::measure(&task)?;
        ProductArtifactReference::measure(&status)?;
        for value in [&diff, &gates, &review, &summary] {
            if !value.is_empty() {
                ProductArtifactReference::measure(value)?;
            }
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
            operation,
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

    /// Authoritative operation knowledge and controls for this observation.
    #[must_use]
    pub const fn operation(&self) -> &ProductRunOperation {
        &self.operation
    }

    /// Replaces only the read-only operation projection.
    #[must_use]
    pub fn with_operation(mut self, operation: ProductRunOperation) -> Self {
        self.operation = operation;
        self
    }

    /// Durable handoff for the exact candidate when one exists.
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
