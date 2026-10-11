//! Exact time-independent revision identity shared across authority decisions.

use crate::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    WorkspaceId,
};

#[doc = " The complete immutable revision identity to which authority and evidence are bound."]
#[doc = ""]
#[doc = " `PolicyId` is the sole policy identity in the tuple. Changing any component creates a distinct"]
#[doc = " authority context and invalidates values bound to the prior tuple."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RevisionTuple {
    acceptance_spec_id: AcceptanceSpecId,
    harness_id: HarnessId,
    workspace_id: WorkspaceId,
    workspace_generation: Generation,
    workspace_revision: RevisionNumber,
    policy_id: PolicyId,
    provider_profile_id: ProviderProfileId,
}
impl RevisionTuple {
    #[doc = " Creates an exact revision tuple from already validated nominal components."]
    #[must_use]
    pub const fn new(
        acceptance_spec_id: AcceptanceSpecId,
        harness_id: HarnessId,
        workspace_id: WorkspaceId,
        workspace_generation: Generation,
        workspace_revision: RevisionNumber,
        policy_id: PolicyId,
        provider_profile_id: ProviderProfileId,
    ) -> Self {
        Self {
            acceptance_spec_id,
            harness_id,
            workspace_id,
            workspace_generation,
            workspace_revision,
            policy_id,
            provider_profile_id,
        }
    }
    #[doc = " Returns the governing acceptance-specification identity."]
    #[must_use]
    pub const fn acceptance_spec_id(&self) -> AcceptanceSpecId {
        self.acceptance_spec_id
    }
    #[doc = " Returns the governing immutable harness identity."]
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    #[doc = " Returns the isolated workspace lineage identity."]
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    #[doc = " Returns the mutation-fencing workspace generation."]
    #[must_use]
    pub const fn workspace_generation(&self) -> Generation {
        self.workspace_generation
    }
    #[doc = " Returns the immutable workspace revision within the generation."]
    #[must_use]
    pub const fn workspace_revision(&self) -> RevisionNumber {
        self.workspace_revision
    }
    #[doc = " Returns the sole immutable policy identity."]
    #[must_use]
    pub const fn policy_id(&self) -> PolicyId {
        self.policy_id
    }
    #[doc = " Returns the immutable provider-profile identity."]
    #[must_use]
    pub const fn provider_profile_id(&self) -> ProviderProfileId {
        self.provider_profile_id
    }
}
