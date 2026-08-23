//! Complete actor, role, environment, permission, revision, time, and use scopes.

use crate::{ActorRole, PermissionSet, UseLimit, ValidityWindow};
use peritus_types::{
    ActorId, CapabilityName, EnvironmentId, ResourceId, RevisionTuple,
};
use vstd::prelude::*;

verus! {

/// Exact actor, role, environment, permissions, revision, validity, and use scope.
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
    /// Returns the exact actor value used by specifications.
    pub closed spec fn spec_actor(&self) -> ActorId { self.actor }

    /// Returns the exact actor identifier bytes used by specifications.
    pub closed spec fn spec_actor_id(&self) -> [u8; 16] { self.actor.spec_bytes() }

    /// Returns the exact stable role used by specifications.
    pub closed spec fn spec_role(&self) -> ActorRole { self.role }

    /// Returns the exact environment identifier bytes used by specifications.
    pub closed spec fn spec_environment_id(&self) -> [u8; 16] {
        self.environment.spec_bytes()
    }

    /// Returns the exact environment value used by specifications.
    pub closed spec fn spec_environment(&self) -> EnvironmentId { self.environment }

    /// Returns the exact ordered permission sequence used by specifications.
    pub closed spec fn spec_permissions(&self) -> Seq<crate::Permission> {
        self.permissions.spec_values()
    }

    /// Returns exact comparator-based permission membership used by specifications.
    pub closed spec fn spec_contains_permission(&self, permission: &crate::Permission) -> bool {
        self.permissions.spec_contains(permission)
    }

    pub(crate) proof fn contained_permission_is_one_exact_pair(
        &self,
        permission: &crate::Permission,
    )
        requires self.spec_contains_permission(permission),
        ensures
            exists |index: int| 0 <= index < self.spec_permissions().len()
                && #[trigger] self.spec_permissions()[index].spec_resource_id()
                    == permission.spec_resource_id()
                && self.spec_permissions()[index].spec_capability_name()
                    == permission.spec_capability_name(),
    {
        reveal(CapabilityScope::spec_contains_permission);
        reveal(CapabilityScope::spec_permissions);
        self.permissions.contained_value_is_one_exact_pair(permission);
        assert(self.spec_permissions() == self.permissions.spec_values());
        assert(exists |index: int| 0 <= index < self.permissions.spec_values().len()
            && #[trigger] self.permissions.spec_values()[index].spec_resource_id()
                == permission.spec_resource_id()
            && self.permissions.spec_values()[index].spec_capability_name()
                == permission.spec_capability_name());
    }

    /// Returns the exact revision tuple used by specifications.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Returns the sole exact policy identifier bytes through the revision tuple.
    pub closed spec fn spec_policy_id(&self) -> [u8; 16] {
        self.revision.spec_policy_id().spec_bytes()
    }

    /// Returns the exact validity interval used by specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.validity }

    /// Returns the exact use bound used by specifications.
    pub closed spec fn spec_use_limit(&self) -> UseLimit { self.use_limit }

    /// Creates one complete checked capability scope.
    #[must_use]
    pub const fn new(
        actor: ActorId,
        role: ActorRole,
        environment: EnvironmentId,
        permissions: PermissionSet,
        revision: RevisionTuple,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> (scope: Self)
        ensures
            scope.spec_actor_id() == actor.spec_bytes(),
            scope.spec_role() == role,
            scope.spec_environment_id() == environment.spec_bytes(),
            scope.spec_permissions() == permissions.spec_values(),
            scope.spec_revision() == revision,
            scope.spec_validity() == validity,
            scope.spec_use_limit() == use_limit,
    {
        Self { actor, role, environment, permissions, revision, validity, use_limit }
    }

    /// Returns the exact actor.
    #[must_use]
    pub const fn actor(&self) -> (actor: ActorId)
        ensures actor == self.spec_actor(),
    { self.actor }

    /// Returns the exact actor identity.
    #[must_use]
    pub const fn actor_id(&self) -> (actor_id: ActorId)
        ensures actor_id.spec_bytes() == self.spec_actor_id(),
    { self.actor }

    /// Returns the exact role.
    #[must_use]
    pub const fn role(&self) -> (role: ActorRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Returns the exact environment.
    #[must_use]
    pub const fn environment(&self) -> (environment: EnvironmentId)
        ensures environment == self.spec_environment(),
    { self.environment }

    /// Returns the exact environment identity.
    #[must_use]
    pub const fn environment_id(&self) -> (environment_id: EnvironmentId)
        ensures environment_id.spec_bytes() == self.spec_environment_id(),
    { self.environment }

    /// Returns the exact canonical permission pairs.
    #[must_use]
    pub const fn permissions(&self) -> (permissions: &PermissionSet)
        ensures permissions.spec_values() == self.spec_permissions(),
    { &self.permissions }

    /// Returns whether the scope contains one exact resource and capability-name pair.
    #[must_use]
    pub fn contains_permission(
        &self,
        resource_id: ResourceId,
        capability_name: &CapabilityName,
    ) -> bool {
        let values = self.permissions.as_slice();
        let mut index = 0;
        while index < values.len()
            invariant 0 <= index <= values.len(),
            decreases values.len() - index,
        {
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

    pub(crate) fn contains_permission_exact(
        &self,
        permission: &crate::Permission,
    ) -> (result: bool)
        ensures result == self.spec_contains_permission(permission),
    {
        self.permissions.contains(permission)
    }

    /// Returns the exact immutable revision tuple.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision(),
    { self.revision }

    /// Returns the sole policy identity through the revision tuple.
    #[must_use]
    pub const fn policy_id(&self) -> (policy_id: peritus_types::PolicyId)
        ensures policy_id.spec_bytes() == self.spec_policy_id(),
    { self.revision.policy_id() }

    /// Returns the half-open validity interval.
    #[must_use]
    pub const fn validity(&self) -> (validity: ValidityWindow)
        ensures validity == self.spec_validity(),
    { self.validity }

    /// Returns the requested/effective use bound.
    #[must_use]
    pub const fn use_limit(&self) -> (use_limit: UseLimit)
        ensures use_limit == self.spec_use_limit(),
    { self.use_limit }

    pub(crate) fn with_constraints(
        self,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> (scope: Self)
        ensures
            scope.spec_actor_id() == self.spec_actor_id(),
            scope.spec_role() == self.spec_role(),
            scope.spec_environment_id() == self.spec_environment_id(),
            scope.spec_permissions() == self.spec_permissions(),
            scope.spec_revision() == self.spec_revision(),
            scope.spec_validity() == validity,
            scope.spec_use_limit() == use_limit,
    {
        Self { validity, use_limit, ..self }
    }
}

/// Whole-request policy query for one complete exact scope.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorizationRequest {
    scope: CapabilityScope,
}

impl AuthorizationRequest {
    /// Returns the complete requested scope used by policy specifications.
    pub closed spec fn spec_scope_value(&self) -> CapabilityScope { self.scope }
    /// Returns the exact actor identifier bytes used by specifications.
    pub closed spec fn spec_actor_id(&self) -> [u8; 16] { self.scope.spec_actor_id() }

    /// Returns the exact stable role used by specifications.
    pub closed spec fn spec_role(&self) -> ActorRole { self.scope.spec_role() }

    /// Returns the exact environment identifier bytes used by specifications.
    pub closed spec fn spec_environment_id(&self) -> [u8; 16] {
        self.scope.spec_environment_id()
    }

    /// Returns the exact permission sequence used by specifications.
    pub closed spec fn spec_permissions(&self) -> Seq<crate::Permission> {
        self.scope.spec_permissions()
    }

    /// Returns the exact revision tuple used by specifications.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.scope.spec_revision() }

    /// Returns the exact requested validity used by specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.scope.spec_validity() }

    /// Returns the exact requested use bound used by specifications.
    pub closed spec fn spec_use_limit(&self) -> UseLimit { self.scope.spec_use_limit() }

    /// Creates a whole-request query from one checked exact scope.
    #[must_use]
    pub const fn new(scope: CapabilityScope) -> (request: Self)
        ensures
            request.spec_scope_value() == scope,
            request.spec_actor_id() == scope.spec_actor_id(),
            request.spec_role() == scope.spec_role(),
            request.spec_environment_id() == scope.spec_environment_id(),
            request.spec_permissions() == scope.spec_permissions(),
            request.spec_revision() == scope.spec_revision(),
            request.spec_validity() == scope.spec_validity(),
            request.spec_use_limit() == scope.spec_use_limit(),
    { Self { scope } }

    /// Borrows the complete requested scope.
    #[must_use]
    pub const fn scope(&self) -> (scope: &CapabilityScope)
        ensures
            scope == self.spec_scope_value(),
            scope.spec_actor_id() == self.spec_actor_id(),
            scope.spec_role() == self.spec_role(),
            scope.spec_environment_id() == self.spec_environment_id(),
            scope.spec_permissions() == self.spec_permissions(),
            scope.spec_revision() == self.spec_revision(),
            scope.spec_validity() == self.spec_validity(),
            scope.spec_use_limit() == self.spec_use_limit(),
    { &self.scope }

    pub(crate) fn into_scope(self) -> (scope: CapabilityScope)
        ensures
            scope == self.spec_scope_value(),
            scope.spec_actor_id() == self.spec_actor_id(),
            scope.spec_role() == self.spec_role(),
            scope.spec_environment_id() == self.spec_environment_id(),
            scope.spec_permissions() == self.spec_permissions(),
            scope.spec_revision() == self.spec_revision(),
            scope.spec_validity() == self.spec_validity(),
            scope.spec_use_limit() == self.spec_use_limit(),
    { self.scope }
}

} // verus!
