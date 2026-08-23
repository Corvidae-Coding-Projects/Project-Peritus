//! Complete parent-relative scope selectors and exact executable refinement.

use crate::{
    identity::revision_values_equal,
    ActorSelector, CapabilityScope, EnvironmentSelector, Permission, PermissionSelector,
    RoleSelector,
};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

mod containment;

verus! {

/// Complete parent-relative selector with one exact revision tuple.
#[derive(Debug, Eq, PartialEq)]
pub struct ScopeSelector {
    pub(crate) actors: ActorSelector,
    pub(crate) roles: RoleSelector,
    pub(crate) environments: EnvironmentSelector,
    pub(crate) permissions: PermissionSelector,
    pub(crate) revision: RevisionTuple,
}

impl ScopeSelector {
    /// Returns the exact checked selector components used by constructor contracts.
    pub closed spec fn spec_actor_selector(&self) -> ActorSelector { self.actors }
    /// Returns the exact checked role selector used by constructor contracts.
    pub closed spec fn spec_role_selector(&self) -> RoleSelector { self.roles }
    /// Returns the exact checked environment selector used by constructor contracts.
    pub closed spec fn spec_environment_selector(&self) -> EnvironmentSelector {
        self.environments
    }
    /// Returns the exact checked permission selector used by constructor contracts.
    pub closed spec fn spec_permission_selector(&self) -> PermissionSelector {
        self.permissions
    }
    /// Returns the exact selector revision used by constructor contracts.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }
    /// Returns whether this selector is the exact revision-only rebind of another selector.
    pub closed spec fn spec_is_revision_rebind_of(
        &self,
        original: &Self,
        revision: RevisionTuple,
    ) -> bool {
        self.actors.spec_same_as(&original.actors)
            && self.roles.spec_same_as(&original.roles)
            && self.environments.spec_same_as(&original.environments)
            && self.permissions.spec_same_as(&original.permissions)
            && self.revision == revision
    }

    /// Returns exact identity-dimension matching used by policy specifications.
    pub closed spec fn spec_matches_identity(&self, scope: &CapabilityScope) -> bool {
        self.actors.spec_contains(scope.spec_actor())
            && self.roles.spec_contains(scope.spec_role())
            && self.environments.spec_contains(scope.spec_environment())
            && crate::model::same_revision(self.revision, scope.spec_revision())
    }

    /// Returns whether at least one requested exact permission matches this selector.
    pub closed spec fn spec_matches_any_permission(&self, scope: &CapabilityScope) -> bool {
        exists |index: int| 0 <= index < scope.spec_permissions().len()
            && #[trigger] self.permissions.spec_contains(&scope.spec_permissions()[index])
    }

    /// Returns exact permission membership used by policy specifications.
    pub closed spec fn spec_contains_permission(&self, permission: &Permission) -> bool {
        self.permissions.spec_contains(permission)
    }

    /// Creates a selector from checked dimensions and one exact revision tuple.
    #[must_use]
    pub const fn new(
        actors: ActorSelector,
        roles: RoleSelector,
        environments: EnvironmentSelector,
        permissions: PermissionSelector,
        revision: RevisionTuple,
    ) -> (selector: Self)
        ensures
            selector.spec_actor_selector() == actors,
            selector.spec_role_selector() == roles,
            selector.spec_environment_selector() == environments,
            selector.spec_permission_selector() == permissions,
            selector.spec_revision() == revision,
    {
        Self { actors, roles, environments, permissions, revision }
    }

    /// Returns the actor selector.
    #[must_use]
    pub const fn actors(&self) -> &ActorSelector { &self.actors }

    /// Returns the role selector.
    #[must_use]
    pub const fn roles(&self) -> &RoleSelector { &self.roles }

    /// Returns the environment selector.
    #[must_use]
    pub const fn environments(&self) -> &EnvironmentSelector { &self.environments }

    /// Returns the permission selector.
    #[must_use]
    pub const fn permissions(&self) -> &PermissionSelector { &self.permissions }

    /// Returns the exact revision tuple.
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple { &self.revision }

    pub(crate) fn matches_identity(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_matches_identity(scope),
    {
        self.actors.contains(scope.actor())
            && self.roles.contains(scope.role())
            && self.environments.contains(scope.environment())
            && revision_values_equal(self.revision, scope.revision())
    }

    pub(crate) fn matches_any_permission(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_matches_any_permission(scope),
    {
        let permissions = scope.permissions();
        let values = permissions.as_slice();
        let mut index = 0;
        while index < values.len()
            invariant
                0 <= index <= values.len(),
                values@ == permissions.spec_values(),
                permissions.spec_values() == scope.spec_permissions(),
                forall |prior: int| 0 <= prior < index ==>
                    !#[trigger] self.permissions.spec_contains(&permissions.spec_values()[prior]),
            decreases values.len() - index,
        {
            if self.permissions.contains(&values[index]) {
                return true;
            }
            index += 1;
        }
        false
    }

    pub(crate) fn contains_permission(&self, permission: &Permission) -> (result: bool)
        ensures result == self.spec_contains_permission(permission),
    {
        self.permissions.contains(permission)
    }

    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> (rebound: Self)
        ensures rebound.spec_is_revision_rebind_of(self, revision),
    {
        let rebound = Self {
            actors: self.actors.duplicate(),
            roles: self.roles.duplicate(),
            environments: self.environments.duplicate(),
            permissions: self.permissions.duplicate(),
            revision,
        };
        reveal(ScopeSelector::spec_is_revision_rebind_of);
        rebound
    }
}

} // verus!
