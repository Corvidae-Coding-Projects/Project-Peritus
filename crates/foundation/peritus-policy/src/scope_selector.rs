//! Complete parent-relative scope selectors and exact executable refinement.

use crate::{
    ActorSelector, CapabilityScope, EnvironmentSelector, Permission, PermissionSelector,
    RoleSelector, identity::revision_values_equal,
};
use peritus_types::RevisionTuple;

mod containment;

#[doc = " Complete parent-relative selector with one exact revision tuple."]
#[derive(Debug, Eq, PartialEq)]
pub struct ScopeSelector {
    pub(crate) actors: ActorSelector,
    pub(crate) roles: RoleSelector,
    pub(crate) environments: EnvironmentSelector,
    pub(crate) permissions: PermissionSelector,
    pub(crate) revision: RevisionTuple,
}
impl ScopeSelector {
    #[doc = " Creates a selector from checked dimensions and one exact revision tuple."]
    #[must_use]
    pub const fn new(
        actors: ActorSelector,
        roles: RoleSelector,
        environments: EnvironmentSelector,
        permissions: PermissionSelector,
        revision: RevisionTuple,
    ) -> Self {
        Self { actors, roles, environments, permissions, revision }
    }
    #[doc = " Returns the actor selector."]
    #[must_use]
    pub const fn actors(&self) -> &ActorSelector {
        &self.actors
    }
    #[doc = " Returns the role selector."]
    #[must_use]
    pub const fn roles(&self) -> &RoleSelector {
        &self.roles
    }
    #[doc = " Returns the environment selector."]
    #[must_use]
    pub const fn environments(&self) -> &EnvironmentSelector {
        &self.environments
    }
    #[doc = " Returns the permission selector."]
    #[must_use]
    pub const fn permissions(&self) -> &PermissionSelector {
        &self.permissions
    }
    #[doc = " Returns the exact revision tuple."]
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    pub(crate) fn matches_identity(&self, scope: &CapabilityScope) -> bool {
        self.actors.contains(scope.actor())
            && self.roles.contains(scope.role())
            && self.environments.contains(scope.environment())
            && revision_values_equal(self.revision, scope.revision())
    }
    pub(crate) fn matches_any_permission(&self, scope: &CapabilityScope) -> bool {
        let permissions = scope.permissions();
        let values = permissions.as_slice();
        let mut index = 0;
        while index < values.len() {
            if self.permissions.contains(&values[index]) {
                return true;
            }
            index += 1;
        }
        false
    }
    pub(crate) fn contains_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }
    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> Self {
        Self {
            actors: self.actors.duplicate(),
            roles: self.roles.duplicate(),
            environments: self.environments.duplicate(),
            permissions: self.permissions.duplicate(),
            revision,
        }
    }
}
