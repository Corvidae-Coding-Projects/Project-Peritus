//! Complete actor, role, environment, permission, revision, time, and use scopes.

use crate::{ActorRole, PermissionSet, UseLimit, ValidityWindow};
use peritus_types::{ActorId, CapabilityName, EnvironmentId, ResourceId, RevisionTuple};

#[doc = " Exact actor, role, environment, permissions, revision, validity, and use scope."]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityScope {
    actor: ActorId,
    role: ActorRole,
    environment: EnvironmentId,
    permissions: PermissionSet,
    revision: RevisionTuple,
    validity: ValidityWindow,
    use_limit: UseLimit,
}
impl CapabilityScope {
    #[doc = " Creates one complete checked capability scope."]
    #[must_use]
    pub const fn new(
        actor: ActorId,
        role: ActorRole,
        environment: EnvironmentId,
        permissions: PermissionSet,
        revision: RevisionTuple,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> Self {
        Self { actor, role, environment, permissions, revision, validity, use_limit }
    }
    #[doc = " Returns the exact actor."]
    #[must_use]
    pub const fn actor(&self) -> ActorId {
        self.actor
    }
    #[doc = " Returns the exact actor identity."]
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor
    }
    #[doc = " Returns the exact role."]
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    #[doc = " Returns the exact environment."]
    #[must_use]
    pub const fn environment(&self) -> EnvironmentId {
        self.environment
    }
    #[doc = " Returns the exact environment identity."]
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment
    }
    #[doc = " Returns the exact canonical permission pairs."]
    #[must_use]
    pub const fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }
    #[doc = " Returns whether the scope contains one exact resource and capability-name pair."]
    #[must_use]
    pub fn contains_permission(
        &self,
        resource_id: ResourceId,
        capability_name: &CapabilityName,
    ) -> bool {
        let values = self.permissions.as_slice();
        let mut index = 0;
        while index < values.len() {
            let permission = &values[index];
            if permission.resource_id() == resource_id
                && permission.capability_name() == capability_name
            {
                return true;
            }
            index += 1;
        }
        false
    }
    pub(crate) fn contains_permission_exact(&self, permission: &crate::Permission) -> bool {
        self.permissions.contains(permission)
    }
    #[doc = " Returns the exact immutable revision tuple."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the sole policy identity through the revision tuple."]
    #[must_use]
    pub const fn policy_id(&self) -> peritus_types::PolicyId {
        self.revision.policy_id()
    }
    #[doc = " Returns the half-open validity interval."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    #[doc = " Returns the requested/effective use bound."]
    #[must_use]
    pub const fn use_limit(&self) -> UseLimit {
        self.use_limit
    }
    pub(crate) fn with_constraints(self, validity: ValidityWindow, use_limit: UseLimit) -> Self {
        Self { validity, use_limit, ..self }
    }
}
#[doc = " Whole-request policy query for one complete exact scope."]
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorizationRequest {
    scope: CapabilityScope,
}
impl AuthorizationRequest {
    #[doc = " Creates a whole-request query from one checked exact scope."]
    #[must_use]
    pub const fn new(scope: CapabilityScope) -> Self {
        Self { scope }
    }
    #[doc = " Borrows the complete requested scope."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
    pub(crate) fn into_scope(self) -> CapabilityScope {
        self.scope
    }
}
