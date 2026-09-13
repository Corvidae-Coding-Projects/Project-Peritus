//! Checked projection of an exact B1 capability-use transition.

use crate::{KernelError, KernelErrorKind};
use peritus_policy::{ActorRole, CapabilityUseTransition};
use peritus_types::{
    ActionId, ActorId, CapabilityName, EnvironmentId, ResourceId, RevisionTuple, Sha256Digest,
};
use vstd::prelude::*;

verus! {

/// Non-authorizing record that B1 checked one exact action-bound capability use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionAuthorizationWitness {
    transition_digest: Sha256Digest,
    resource_id: ResourceId,
    capability_name: CapabilityName,
}

impl ActionAuthorizationWitness {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_transition(
        action_id: ActionId,
        action_digest: Sha256Digest,
        actor_id: ActorId,
        role: ActorRole,
        environment_id: EnvironmentId,
        revision: RevisionTuple,
        transition: &CapabilityUseTransition,
    ) -> Result<Self, KernelError> {
        let scope = transition.scope();
        if transition.action_id() != action_id
            || transition.action_digest() != action_digest
            || scope.actor_id() != actor_id
            || scope.role() != role
            || scope.environment_id() != environment_id
            || scope.revision() != revision
        {
            return Err(KernelError::new(KernelErrorKind::AuthorityMismatch));
        }
        Ok(Self {
            transition_digest: transition.transition_digest(),
            resource_id: transition.permission().resource_id(),
            capability_name: transition.permission().capability_name().clone(),
        })
    }

    /// Returns the exact B1 logical transition digest.
    #[must_use]
    pub const fn transition_digest(&self) -> Sha256Digest { self.transition_digest }
    /// Returns the authorized resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId { self.resource_id }
    /// Returns the authorized capability name.
    #[must_use]
    pub const fn capability_name(&self) -> &CapabilityName { &self.capability_name }
}

} // verus!
