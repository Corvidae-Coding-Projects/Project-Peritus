//! Bounded product-run observation snapshots.

use peritus_types::{RunId, WorkspaceId};

use super::{
    MAX_PRODUCT_DETAIL_BYTES, MAX_PRODUCT_TASK_BYTES, ProductDeliverable, ProductProviderSelection,
    ProductRunMessageError, ProductRunPhase, bounded_text, optional_bounded_text,
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
