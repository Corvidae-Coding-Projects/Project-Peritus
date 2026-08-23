//! Complete finite parent authority boundaries.

use crate::{
    identity::{
        actor_values_contain, environment_values_contain, revision_values_equal,
        role_values_contain,
    },
    scope_validation::{validate_actor_values, validate_environment_values, validate_role_values},
    ActorRole, CapabilityScope, PermissionSet, PolicyError, UseLimit, ValidityWindow,
};
#[cfg(verus_only)]
use crate::{
    identity::{
        actor_values_spec_contains, environment_values_spec_contains, role_values_spec_contains,
    },
    scope_validation::{
        actor_validation_error, environment_validation_error, role_validation_error,
    },
    CanonicalCollection,
};
use peritus_types::{ActorId, EnvironmentId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// Returns the exact first canonical identity-collection failure for a boundary.
pub open spec fn boundary_validation_error(
    actors: Seq<ActorId>,
    roles: Seq<ActorRole>,
    environments: Seq<EnvironmentId>,
) -> Option<(crate::PolicyErrorKind, CanonicalCollection)> {
    if actor_validation_error(actors) is Some {
        Some((actor_validation_error(actors).unwrap(), CanonicalCollection::Actors))
    } else if role_validation_error(roles) is Some {
        Some((role_validation_error(roles).unwrap(), CanonicalCollection::Roles))
    } else if environment_validation_error(environments) is Some {
        Some((environment_validation_error(environments).unwrap(), CanonicalCollection::Environments))
    } else {
        None
    }
}

/// Complete exact parent boundary that gives `AnyWithinParent` its finite meaning.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorityBoundary {
    actors: Vec<ActorId>,
    roles: Vec<ActorRole>,
    environments: Vec<EnvironmentId>,
    permissions: PermissionSet,
    revision: RevisionTuple,
    validity: ValidityWindow,
    use_limit: UseLimit,
}

impl AuthorityBoundary {
    /// Returns the exact actor identity sequence used by constructor specifications.
    pub closed spec fn spec_actors(&self) -> Seq<ActorId> { self.actors@ }

    /// Returns the exact role sequence used by constructor specifications.
    pub closed spec fn spec_roles(&self) -> Seq<ActorRole> { self.roles@ }

    /// Returns the exact environment identity sequence used by constructor specifications.
    pub closed spec fn spec_environments(&self) -> Seq<EnvironmentId> { self.environments@ }

    /// Returns the exact permission set used by constructor specifications.
    pub closed spec fn spec_permissions(&self) -> Seq<crate::Permission> {
        self.permissions.spec_values()
    }

    pub(crate) closed spec fn spec_contains_actor(&self, actor: ActorId) -> bool {
        actor_values_spec_contains(self.actors@, actor)
    }

    pub(crate) closed spec fn spec_contains_role(&self, role: ActorRole) -> bool {
        role_values_spec_contains(self.roles@, role)
    }

    pub(crate) closed spec fn spec_contains_environment(
        &self,
        environment: EnvironmentId,
    ) -> bool {
        environment_values_spec_contains(self.environments@, environment)
    }

    pub(crate) closed spec fn spec_contains_permission(&self, permission: &crate::Permission) -> bool {
        self.permissions.spec_contains(permission)
    }

    /// Returns whether this boundary preserves every authority dimension under a revision rebind.
    pub closed spec fn spec_is_revision_rebind_of(
        &self,
        original: &Self,
        revision: RevisionTuple,
    ) -> bool {
        self.actors@ == original.actors@
            && self.roles@ == original.roles@
            && self.environments@ == original.environments@
            && self.permissions.spec_same_as(&original.permissions)
            && self.revision == revision
            && self.validity == original.validity
            && self.use_limit == original.use_limit
    }

    pub(crate) proof fn revision_rebind_has_exact_revision(
        &self,
        original: &Self,
        revision: RevisionTuple,
    )
        requires self.spec_is_revision_rebind_of(original, revision),
        ensures self.spec_revision() == revision,
    {
        reveal(AuthorityBoundary::spec_is_revision_rebind_of);
        reveal(AuthorityBoundary::spec_revision);
    }
    /// Returns exact complete boundary containment used by evaluation specifications.
    pub closed spec fn spec_contains_scope(&self, scope: &CapabilityScope) -> bool {
        actor_values_spec_contains(self.actors@, scope.spec_actor())
            && role_values_spec_contains(self.roles@, scope.spec_role())
            && environment_values_spec_contains(self.environments@, scope.spec_environment())
            && (forall |index: int| 0 <= index < scope.spec_permissions().len() ==>
                #[trigger] self.permissions.spec_contains(&scope.spec_permissions()[index]))
            && crate::model::same_revision(self.revision, scope.spec_revision())
            && scope.spec_validity().spec_is_within(self.validity)
            && scope.spec_use_limit().spec_is_within(self.use_limit)
    }

    /// Returns the exact parent validity bound used by evaluation specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.validity }

    /// Returns the exact boundary revision used by amendment specifications.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Returns the exact parent use bound used by evaluation specifications.
    pub closed spec fn spec_use_limit(&self) -> UseLimit { self.use_limit }

    /// Creates a complete canonical authority boundary.
    ///
    /// # Errors
    ///
    /// Returns a precise canonical actor, role, or environment failure.
    pub fn new(
        actors: Vec<ActorId>,
        roles: Vec<ActorRole>,
        environments: Vec<EnvironmentId>,
        permissions: PermissionSet,
        revision: RevisionTuple,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(boundary) => {
                    boundary_validation_error(actors@, roles@, environments@).is_none()
                        && boundary.spec_actors() == actors@
                        && boundary.spec_roles() == roles@
                        && boundary.spec_environments() == environments@
                        && boundary.spec_permissions() == permissions.spec_values()
                        && boundary.spec_revision() == revision
                        && boundary.spec_validity() == validity
                        && boundary.spec_use_limit() == use_limit
                }
                Err(error) => {
                    boundary_validation_error(actors@, roles@, environments@)
                        == Some((error.spec_kind(), error.spec_collection().unwrap()))
                        && error.spec_collection().is_some()
                        && error.spec_dimension().is_none()
                }
            },
    {
        validate_actor_values(actors.as_slice())?;
        validate_role_values(roles.as_slice())?;
        validate_environment_values(environments.as_slice())?;
        let boundary = Self { actors, roles, environments, permissions, revision, validity, use_limit };
        reveal(AuthorityBoundary::spec_actors);
        reveal(AuthorityBoundary::spec_roles);
        reveal(AuthorityBoundary::spec_environments);
        reveal(AuthorityBoundary::spec_permissions);
        reveal(AuthorityBoundary::spec_revision);
        reveal(AuthorityBoundary::spec_validity);
        reveal(AuthorityBoundary::spec_use_limit);
        Ok(boundary)
    }

    /// Borrows canonical actor identities.
    #[must_use]
    pub const fn actors(&self) -> &[ActorId] { self.actors.as_slice() }

    /// Borrows canonical stable roles.
    #[must_use]
    pub const fn roles(&self) -> &[ActorRole] { self.roles.as_slice() }

    /// Borrows canonical environment identities.
    #[must_use]
    pub const fn environments(&self) -> &[EnvironmentId] { self.environments.as_slice() }

    /// Returns the exact parent permission pairs.
    #[must_use]
    pub const fn permissions(&self) -> &PermissionSet { &self.permissions }

    /// Returns the exact parent revision tuple.
    #[must_use]
    pub const fn revision(&self) -> (revision: &RevisionTuple)
        ensures *revision == self.spec_revision(),
    {
        &self.revision
    }

    /// Returns the parent validity bound.
    #[must_use]
    pub const fn validity(&self) -> (validity: ValidityWindow)
        ensures validity == self.spec_validity(),
    { self.validity }

    /// Returns the parent use bound.
    #[must_use]
    pub const fn use_limit(&self) -> (use_limit: UseLimit)
        ensures use_limit == self.spec_use_limit(),
    { self.use_limit }

    pub(crate) fn contains_actor(&self, actor: ActorId) -> (contains: bool)
        ensures contains == self.spec_contains_actor(actor),
    {
        actor_values_contain(self.actors.as_slice(), actor)
    }

    pub(crate) fn contains_role(&self, role: ActorRole) -> (contains: bool)
        ensures contains == self.spec_contains_role(role),
    {
        role_values_contain(self.roles.as_slice(), role)
    }

    pub(crate) fn contains_environment(&self, environment: EnvironmentId) -> (contains: bool)
        ensures contains == self.spec_contains_environment(environment),
    {
        environment_values_contain(self.environments.as_slice(), environment)
    }

    pub(crate) fn contains_permission(&self, permission: &crate::Permission) -> (contains: bool)
        ensures contains == self.spec_contains_permission(permission),
    {
        self.permissions.contains(permission)
    }

    pub(crate) fn contains_scope(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_contains_scope(scope),
    {
        reveal(AuthorityBoundary::spec_contains_scope);
        let actor = scope.actor();
        let role = scope.role();
        let environment = scope.environment();
        let permissions = scope.permissions();
        let revision = scope.revision();
        let actor_contained = actor_values_contain(self.actors.as_slice(), actor);
        let role_contained = role_values_contain(self.roles.as_slice(), role);
        let environment_contained =
            environment_values_contain(self.environments.as_slice(), environment);
        let permissions_contained = permissions.is_subset_of(&self.permissions);
        let revision_matches = revision_values_equal(self.revision, revision);
        let validity_contained = scope.validity().is_within(self.validity);
        let use_limit_contained = scope.use_limit().is_within(self.use_limit);
        assert(actor_contained == actor_values_spec_contains(self.actors@, scope.spec_actor()));
        assert(role_contained == role_values_spec_contains(self.roles@, scope.spec_role()));
        assert(environment_contained
            == environment_values_spec_contains(self.environments@, scope.spec_environment()));
        assert(permissions_contained == (
            forall |index: int| 0 <= index < scope.spec_permissions().len() ==>
                #[trigger] self.permissions.spec_contains(&scope.spec_permissions()[index])
        ));
        assert(revision_matches
            == crate::model::same_revision(self.revision, scope.spec_revision()));
        let contained = actor_contained
            && role_contained
            && environment_contained
            && permissions_contained
            && revision_matches
            && validity_contained
            && use_limit_contained;
        assert(contained == self.spec_contains_scope(scope));
        contained
    }

    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> (rebound: Self)
        ensures rebound.spec_is_revision_rebind_of(self, revision),
    {
        let mut actors = Vec::new();
        let mut index = 0;
        while index < self.actors.len()
            invariant
                0 <= index <= self.actors.len(),
                actors@ == self.actors@.subrange(0, index as int),
            decreases self.actors.len() - index,
        {
            actors.push(self.actors[index]);
            index += 1;
        }
        let mut roles = Vec::new();
        index = 0;
        while index < self.roles.len()
            invariant
                0 <= index <= self.roles.len(),
                roles@ == self.roles@.subrange(0, index as int),
            decreases self.roles.len() - index,
        {
            roles.push(self.roles[index]);
            index += 1;
        }
        let mut environments = Vec::new();
        index = 0;
        while index < self.environments.len()
            invariant
                0 <= index <= self.environments.len(),
                environments@ == self.environments@.subrange(0, index as int),
            decreases self.environments.len() - index,
        {
            environments.push(self.environments[index]);
            index += 1;
        }
        assert(actors@ == self.actors@);
        assert(roles@ == self.roles@);
        assert(environments@ == self.environments@);
        let rebound = Self {
            actors,
            roles,
            environments,
            permissions: self.permissions.duplicate(),
            revision,
            validity: self.validity,
            use_limit: self.use_limit,
        };
        reveal(AuthorityBoundary::spec_is_revision_rebind_of);
        rebound
    }
}

} // verus!
