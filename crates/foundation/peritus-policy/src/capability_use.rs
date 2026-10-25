//! Exact action-bound capability-use requests.

use crate::{ActorRole, AuthorityInstant, CapabilityScope, Permission, ScopeDimension};
use peritus_types::{ActionId, ActorId, EnvironmentId, RevisionTuple, Sha256Digest};

#[doc = " Exact actor, role, environment, revision, permission, action, digest, and time use request."]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityUseRequest {
    pub(crate) action_id: ActionId,
    pub(crate) action_digest: Sha256Digest,
    pub(crate) permission: Permission,
    pub(crate) actor_id: ActorId,
    pub(crate) role: ActorRole,
    pub(crate) environment_id: EnvironmentId,
    pub(crate) revision: RevisionTuple,
    pub(crate) observed_at: AuthorityInstant,
}
impl CapabilityUseRequest {
    pub(crate) fn into_parts(
        self,
    ) -> (
        ActionId,
        Sha256Digest,
        Permission,
        ActorId,
        ActorRole,
        EnvironmentId,
        RevisionTuple,
        AuthorityInstant,
    ) {
        (
            self.action_id,
            self.action_digest,
            self.permission,
            self.actor_id,
            self.role,
            self.environment_id,
            self.revision,
            self.observed_at,
        )
    }
    pub(crate) const fn actor_matches(&self, scope: &CapabilityScope) -> bool {
        let left = *self.actor_id.as_bytes();
        let scope_actor = scope.actor_id();
        let right = *scope_actor.as_bytes();
        crate::identity::identifier_values_equal(left, right)
    }
    pub(crate) const fn role_matches(&self, scope: &CapabilityScope) -> bool {
        self.role.canonical_rank() == scope.role().canonical_rank()
    }
    pub(crate) const fn environment_matches(&self, scope: &CapabilityScope) -> bool {
        let left = *self.environment_id.as_bytes();
        let scope_environment = scope.environment_id();
        let right = *scope_environment.as_bytes();
        crate::identity::identifier_values_equal(left, right)
    }
    pub(crate) const fn revision_matches(&self, scope: &CapabilityScope) -> bool {
        crate::identity::revision_values_equal(self.revision, scope.revision())
    }
    pub(crate) fn permission_matches(&self, scope: &CapabilityScope) -> bool {
        scope.contains_permission_exact(&self.permission)
    }
    pub(crate) fn scope_mismatch(&self, scope: &CapabilityScope) -> Option<ScopeDimension> {
        if !self.actor_matches(scope) {
            Some(ScopeDimension::Actor)
        } else if !self.role_matches(scope) {
            Some(ScopeDimension::Role)
        } else if !self.environment_matches(scope) {
            Some(ScopeDimension::Environment)
        } else if !self.revision_matches(scope) {
            Some(ScopeDimension::Revision)
        } else if !self.permission_matches(scope) {
            Some(ScopeDimension::Permissions)
        } else {
            None
        }
    }
    #[doc = " Creates a complete exact use request."]
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        action_id: ActionId,
        action_digest: Sha256Digest,
        permission: Permission,
        actor_id: ActorId,
        role: ActorRole,
        environment_id: EnvironmentId,
        revision: RevisionTuple,
        observed_at: AuthorityInstant,
    ) -> Self {
        Self {
            action_id,
            action_digest,
            permission,
            actor_id,
            role,
            environment_id,
            revision,
            observed_at,
        }
    }
    #[doc = " Returns the exact action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    #[doc = " Returns the exact action digest."]
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        self.action_digest
    }
    #[doc = " Returns the exact permission consumed for this action."]
    #[must_use]
    pub const fn permission(&self) -> &Permission {
        &self.permission
    }
    #[doc = " Returns the exact acting principal."]
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    #[doc = " Returns the exact stable actor role."]
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    #[doc = " Returns the exact execution environment."]
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    #[doc = " Returns the complete immutable revision identity."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the authority-time observation for the logical use."]
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
}
