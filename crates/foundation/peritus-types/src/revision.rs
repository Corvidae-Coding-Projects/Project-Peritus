//! Exact time-independent revision identity shared across authority decisions.

use crate::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    WorkspaceId,
};
use vstd::prelude::*;

verus! {

/// The complete immutable revision identity to which authority and evidence are bound.
///
/// `PolicyId` is the sole policy identity in the tuple. Changing any component creates a distinct
/// authority context and invalidates values bound to the prior tuple.
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
    /// Specification view of the governing acceptance-specification identity.
    pub closed spec fn spec_acceptance_spec_id(&self) -> AcceptanceSpecId {
        self.acceptance_spec_id
    }

    /// Specification view of the governing immutable harness identity.
    pub closed spec fn spec_harness_id(&self) -> HarnessId {
        self.harness_id
    }

    /// Specification view of the isolated workspace lineage identity.
    pub closed spec fn spec_workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Specification view of the mutation-fencing workspace generation.
    pub closed spec fn spec_workspace_generation(&self) -> Generation {
        self.workspace_generation
    }

    /// Specification view of the immutable workspace revision within the generation.
    pub closed spec fn spec_workspace_revision(&self) -> RevisionNumber {
        self.workspace_revision
    }

    /// Specification view of the sole immutable policy identity.
    pub closed spec fn spec_policy_id(&self) -> PolicyId {
        self.policy_id
    }

    /// Specification view of the immutable provider-profile identity.
    pub closed spec fn spec_provider_profile_id(&self) -> ProviderProfileId {
        self.provider_profile_id
    }

    /// Creates an exact revision tuple from already validated nominal components.
    #[must_use]
    pub const fn new(
        acceptance_spec_id: AcceptanceSpecId,
        harness_id: HarnessId,
        workspace_id: WorkspaceId,
        workspace_generation: Generation,
        workspace_revision: RevisionNumber,
        policy_id: PolicyId,
        provider_profile_id: ProviderProfileId,
    ) -> (result: Self)
        ensures
            result.spec_acceptance_spec_id() == acceptance_spec_id,
            result.spec_harness_id() == harness_id,
            result.spec_workspace_id() == workspace_id,
            result.spec_workspace_generation() == workspace_generation,
            result.spec_workspace_revision() == workspace_revision,
            result.spec_policy_id() == policy_id,
            result.spec_provider_profile_id() == provider_profile_id,
    {
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

    /// Returns the governing acceptance-specification identity.
    #[must_use]
    pub const fn acceptance_spec_id(&self) -> (result: AcceptanceSpecId)
        ensures result == self.spec_acceptance_spec_id()
    {
        self.acceptance_spec_id
    }

    /// Returns the governing immutable harness identity.
    #[must_use]
    pub const fn harness_id(&self) -> (result: HarnessId)
        ensures result == self.spec_harness_id()
    {
        self.harness_id
    }

    /// Returns the isolated workspace lineage identity.
    #[must_use]
    pub const fn workspace_id(&self) -> (result: WorkspaceId)
        ensures result == self.spec_workspace_id()
    {
        self.workspace_id
    }

    /// Returns the mutation-fencing workspace generation.
    #[must_use]
    pub const fn workspace_generation(&self) -> (result: Generation)
        ensures result == self.spec_workspace_generation()
    {
        self.workspace_generation
    }

    /// Returns the immutable workspace revision within the generation.
    #[must_use]
    pub const fn workspace_revision(&self) -> (result: RevisionNumber)
        ensures result == self.spec_workspace_revision()
    {
        self.workspace_revision
    }

    /// Returns the sole immutable policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> (result: PolicyId)
        ensures result == self.spec_policy_id()
    {
        self.policy_id
    }

    /// Returns the immutable provider-profile identity.
    #[must_use]
    pub const fn provider_profile_id(&self) -> (result: ProviderProfileId)
        ensures result == self.spec_provider_profile_id()
    {
        self.provider_profile_id
    }
}

} // verus!
