//! Explicit run-to-conversation discovery without inferring identities from run identifiers.

use crate::{ProductInteractionSnapshot, ProductRunMessageError, WorkbenchQuery};

/// Read-only run observation and its optional durable workbench input destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductInteractionBinding {
    interaction: ProductInteractionSnapshot,
    conversation: Option<WorkbenchQuery>,
}

impl ProductInteractionBinding {
    /// Binds an observed run to its actual durable conversation, or marks a legacy run.
    ///
    /// # Errors
    /// Rejects a binding outside the run's workspace.
    pub fn new(
        interaction: ProductInteractionSnapshot,
        conversation: Option<WorkbenchQuery>,
    ) -> Result<Self, ProductRunMessageError> {
        if conversation
            .is_some_and(|query| query.workspace() != interaction.snapshot().workspace_id())
        {
            return Err(ProductRunMessageError::InvalidConversationBinding);
        }
        Ok(Self { interaction, conversation })
    }

    /// Observed run and public transcript; opening does not resume execution.
    #[must_use]
    pub const fn interaction(&self) -> &ProductInteractionSnapshot {
        &self.interaction
    }

    /// Exact workbench input destination. None denotes an existing legacy conversation.
    #[must_use]
    pub const fn conversation(&self) -> Option<WorkbenchQuery> {
        self.conversation
    }
}
