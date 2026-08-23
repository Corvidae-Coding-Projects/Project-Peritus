//! Exact action-bound capability-use requests.

use crate::{ActorRole, AuthorityInstant, CapabilityScope, Permission, ScopeDimension};
use peritus_types::{ActionId, ActorId, EnvironmentId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Exact actor, role, environment, revision, permission, action, digest, and time use request.
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
    ) -> (parts: (
        ActionId,
        Sha256Digest,
        Permission,
        ActorId,
        ActorRole,
        EnvironmentId,
        RevisionTuple,
        AuthorityInstant,
    ))
        ensures
            parts.0.spec_bytes() == self.spec_action_id(),
            parts.1.spec_bytes() == self.spec_action_digest(),
            parts.2.spec_resource_id() == self.spec_permission_resource_id(),
            parts.2.spec_capability_name() == self.spec_permission_capability_name(),
            parts.7 == self.spec_observed_at(),
    {
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

    /// Returns the exact action identity bytes used by specifications.
    pub closed spec fn spec_action_id(&self) -> [u8; 16] { self.action_id.spec_bytes() }

    /// Returns the exact action digest bytes used by specifications.
    pub closed spec fn spec_action_digest(&self) -> [u8; 32] {
        self.action_digest.spec_bytes()
    }

    /// Returns the requested permission resource bytes used by specifications.
    pub closed spec fn spec_permission_resource_id(&self) -> [u8; 16] {
        self.permission.spec_resource_id()
    }

    /// Returns the requested permission capability name used by specifications.
    pub closed spec fn spec_permission_capability_name(&self) -> Seq<u8> {
        self.permission.spec_capability_name()
    }

    /// Returns the exact requested permission used by specifications.
    pub closed spec fn spec_permission(&self) -> Permission { self.permission }

    /// Returns the requested actor identity bytes used by specifications.
    pub closed spec fn spec_actor_id(&self) -> [u8; 16] { self.actor_id.spec_bytes() }

    /// Returns the requested stable role used by specifications.
    pub closed spec fn spec_role(&self) -> ActorRole { self.role }

    /// Returns the requested environment identity bytes used by specifications.
    pub closed spec fn spec_environment_id(&self) -> [u8; 16] {
        self.environment_id.spec_bytes()
    }

    /// Returns the requested revision tuple used by specifications.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Returns the exact observed instant used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    pub(crate) const fn actor_matches(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == crate::model::same_identifier(
            self.spec_actor_id(),
            scope.spec_actor_id(),
        ),
    {
        let left = *self.actor_id.as_bytes();
        let scope_actor = scope.actor_id();
        let right = *scope_actor.as_bytes();
        assert(left == self.spec_actor_id());
        assert(right == scope.spec_actor_id());
        crate::identity::identifier_values_equal(left, right)
    }

    pub(crate) const fn role_matches(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == (self.spec_role() == scope.spec_role()),
    {
        self.role.canonical_rank() == scope.role().canonical_rank()
    }

    pub(crate) const fn environment_matches(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == crate::model::same_identifier(
            self.spec_environment_id(),
            scope.spec_environment_id(),
        ),
    {
        let left = *self.environment_id.as_bytes();
        let scope_environment = scope.environment_id();
        let right = *scope_environment.as_bytes();
        assert(left == self.spec_environment_id());
        assert(right == scope.spec_environment_id());
        crate::identity::identifier_values_equal(left, right)
    }

    pub(crate) const fn revision_matches(&self, scope: &CapabilityScope) -> (result: bool)
        ensures
            result == crate::model::same_revision(
                self.spec_revision(),
                scope.spec_revision(),
            ),
    {
        crate::identity::revision_values_equal(self.revision, scope.revision())
    }

    pub(crate) fn permission_matches(&self, scope: &CapabilityScope) -> (result: bool)
        ensures
            result == scope.spec_contains_permission(&self.spec_permission()),
            result ==> exists |index: int| 0 <= index < scope.spec_permissions().len()
                && #[trigger] scope.spec_permissions()[index].spec_resource_id()
                    == self.spec_permission_resource_id()
                && scope.spec_permissions()[index].spec_capability_name()
                    == self.spec_permission_capability_name(),
    {
        let result = scope.contains_permission_exact(&self.permission);
        proof {
            if result {
                scope.contained_permission_is_one_exact_pair(&self.permission);
            }
        }
        result
    }

    pub(crate) fn scope_mismatch(
        &self,
        scope: &CapabilityScope,
    ) -> (dimension: Option<ScopeDimension>)
        ensures
            dimension == crate::capability_use_model::first_scope_mismatch_value(
                self,
                scope,
            ),
            dimension.is_none() == (
                crate::model::same_identifier(self.spec_actor_id(), scope.spec_actor_id())
                    && self.spec_role() == scope.spec_role()
                    && crate::model::same_identifier(
                        self.spec_environment_id(),
                        scope.spec_environment_id(),
                    )
                    && crate::model::same_revision(
                        self.spec_revision(),
                        scope.spec_revision(),
                    )
                    && scope.spec_contains_permission(&self.spec_permission())
            ),
    {
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

    /// Creates a complete exact use request.
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
    ) -> (request: Self)
        ensures
            request.spec_action_id() == action_id.spec_bytes(),
            request.spec_action_digest() == action_digest.spec_bytes(),
            request.spec_permission() == permission,
            request.spec_actor_id() == actor_id.spec_bytes(),
            request.spec_role() == role,
            request.spec_environment_id() == environment_id.spec_bytes(),
            request.spec_revision() == revision,
            request.spec_observed_at() == observed_at,
    {
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

    /// Returns the exact action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.action_id }

    /// Returns the exact action digest.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest { self.action_digest }

    /// Returns the exact permission consumed for this action.
    #[must_use]
    pub const fn permission(&self) -> &Permission { &self.permission }

    /// Returns the exact acting principal.
    #[must_use]
    pub const fn actor_id(&self) -> (actor_id: ActorId)
        ensures actor_id.spec_bytes() == self.spec_actor_id(),
    { self.actor_id }

    /// Returns the exact stable actor role.
    #[must_use]
    pub const fn role(&self) -> (role: ActorRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Returns the exact execution environment.
    #[must_use]
    pub const fn environment_id(&self) -> (environment_id: EnvironmentId)
        ensures environment_id.spec_bytes() == self.spec_environment_id(),
    { self.environment_id }

    /// Returns the complete immutable revision identity.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision(),
    { self.revision }

    /// Returns the authority-time observation for the logical use.
    #[must_use]
    pub const fn observed_at(&self) -> (observed_at: AuthorityInstant)
        ensures observed_at == self.spec_observed_at(),
    { self.observed_at }
}

} // verus!
