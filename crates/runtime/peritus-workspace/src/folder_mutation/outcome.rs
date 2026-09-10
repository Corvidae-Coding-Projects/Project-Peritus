//! Read-only exact folder mutation outcome projection.

use super::{FolderMutationCondition, FolderMutationOutcome};
use peritus_patch::{AppliedPatch, PatchIdentity};
use peritus_types::{ActionId, Generation, ResourceId, RevisionNumber, WorkspaceId};

impl FolderMutationOutcome {
    /// Returns the exact committed action consumed for this effect.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }

    /// Returns the mutated workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Returns the exact authorized resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the generation in which the patch was applied.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Returns the unchanged logical revision named by the committed action.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }

    /// Returns the owner's post-effect readiness; pending cleanup is indeterminate.
    #[must_use]
    pub const fn condition(&self) -> FolderMutationCondition {
        self.condition
    }

    /// Returns the exact applied patch identity.
    #[must_use]
    pub const fn patch_identity(&self) -> PatchIdentity {
        self.patch.identity()
    }

    /// Borrows durable patch transaction evidence, including cleanup readiness.
    #[must_use]
    pub const fn applied_patch(&self) -> &AppliedPatch {
        &self.patch
    }
}
