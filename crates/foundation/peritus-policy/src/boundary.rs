//! Complete finite parent authority boundaries.

use crate::{
    ActorRole, CapabilityScope, PermissionSet, PolicyError, UseLimit, ValidityWindow,
    identity::{
        actor_values_contain, environment_values_contain, revision_values_equal,
        role_values_contain,
    },
    scope_validation::{validate_actor_values, validate_environment_values, validate_role_values},
};

use peritus_types::{ActorId, EnvironmentId, RevisionTuple};

#[doc = " Complete exact parent boundary that gives `AnyWithinParent` its finite meaning."]
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
    #[doc = " Creates a complete canonical authority boundary."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise canonical actor, role, or environment failure."]
    pub fn new(
        actors: Vec<ActorId>,
        roles: Vec<ActorRole>,
        environments: Vec<EnvironmentId>,
        permissions: PermissionSet,
        revision: RevisionTuple,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> Result<Self, PolicyError> {
        validate_actor_values(actors.as_slice())?;
        validate_role_values(roles.as_slice())?;
        validate_environment_values(environments.as_slice())?;
        let boundary =
            Self { actors, roles, environments, permissions, revision, validity, use_limit };
        Ok(boundary)
    }
    #[doc = " Borrows canonical actor identities."]
    #[must_use]
    pub const fn actors(&self) -> &[ActorId] {
        self.actors.as_slice()
    }
    #[doc = " Borrows canonical stable roles."]
    #[must_use]
    pub const fn roles(&self) -> &[ActorRole] {
        self.roles.as_slice()
    }
    #[doc = " Borrows canonical environment identities."]
    #[must_use]
    pub const fn environments(&self) -> &[EnvironmentId] {
        self.environments.as_slice()
    }
    #[doc = " Returns the exact parent permission pairs."]
    #[must_use]
    pub const fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }
    #[doc = " Returns the exact parent revision tuple."]
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    #[doc = " Returns the parent validity bound."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    #[doc = " Returns the parent use bound."]
    #[must_use]
    pub const fn use_limit(&self) -> UseLimit {
        self.use_limit
    }
    pub(crate) fn contains_actor(&self, actor: ActorId) -> bool {
        actor_values_contain(self.actors.as_slice(), actor)
    }
    pub(crate) fn contains_role(&self, role: ActorRole) -> bool {
        role_values_contain(self.roles.as_slice(), role)
    }
    pub(crate) fn contains_environment(&self, environment: EnvironmentId) -> bool {
        environment_values_contain(self.environments.as_slice(), environment)
    }
    pub(crate) fn contains_permission(&self, permission: &crate::Permission) -> bool {
        self.permissions.contains(permission)
    }
    pub(crate) fn contains_scope(&self, scope: &CapabilityScope) -> bool {
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

        actor_contained
            && role_contained
            && environment_contained
            && permissions_contained
            && revision_matches
            && validity_contained
            && use_limit_contained
    }
    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> Self {
        let mut actors = Vec::new();
        let mut index = 0;
        while index < self.actors.len() {
            actors.push(self.actors[index]);
            index += 1;
        }
        let mut roles = Vec::new();
        index = 0;
        while index < self.roles.len() {
            roles.push(self.roles[index]);
            index += 1;
        }
        let mut environments = Vec::new();
        index = 0;
        while index < self.environments.len() {
            environments.push(self.environments[index]);
            index += 1;
        }

        Self {
            actors,
            roles,
            environments,
            permissions: self.permissions.duplicate(),
            revision,
            validity: self.validity,
            use_limit: self.use_limit,
        }
    }
}
