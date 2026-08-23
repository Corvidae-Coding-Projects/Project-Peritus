//! Successful exact action-bound capability-use transitions.

use crate::{
    AuthorityInstant, Capability, CapabilityScope, CapabilityUseRequest, Permission, UseLimit,
};
#[cfg(verus_only)]
use crate::ActorRole;
use peritus_types::{ActionId, Sha256Digest};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Successful move-only logical capability-use transition for one exact action and permission.
///
/// Its fields are intentionally private outside this crate:
///
/// ```compile_fail
/// let _forged = peritus_policy::CapabilityUseTransition {
///     action_id: loop {},
///     action_digest: loop {},
///     permission: loop {},
///     used_at: loop {},
///     transition_digest: loop {},
///     previous_remaining: loop {},
///     successor: loop {},
/// };
/// ```
///
/// A successful transition cannot be cloned or implicitly copied:
///
/// ```compile_fail
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<peritus_policy::CapabilityUseTransition>();
/// ```
///
/// ```compile_fail
/// fn requires_copy<T: Copy>() {}
/// requires_copy::<peritus_policy::CapabilityUseTransition>();
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityUseTransition {
    pub(crate) action_id: ActionId,
    pub(crate) action_digest: Sha256Digest,
    pub(crate) permission: Permission,
    pub(crate) used_at: AuthorityInstant,
    pub(crate) transition_digest: Sha256Digest,
    pub(crate) previous_remaining: UseLimit,
    pub(crate) successor: Capability,
}

impl CapabilityUseTransition {
    pub(crate) fn new(
        request: CapabilityUseRequest,
        transition_digest: Sha256Digest,
        previous_remaining: UseLimit,
        successor: Capability,
    ) -> (transition: Self)
        ensures
            transition.spec_action_id() == request.spec_action_id(),
            transition.spec_action_digest() == request.spec_action_digest(),
            transition.spec_permission_resource_id() == request.spec_permission_resource_id(),
            transition.spec_permission_capability_name()
                == request.spec_permission_capability_name(),
            transition.spec_used_at() == request.spec_observed_at(),
            transition.spec_transition_digest() == transition_digest.spec_bytes(),
            transition.spec_scope_actor_id() == successor.spec_scope_actor_id(),
            transition.spec_scope_role() == successor.spec_scope_role(),
            transition.spec_scope_environment_id() == successor.spec_scope_environment_id(),
            transition.spec_scope_permissions() == successor.spec_scope_permissions(),
            transition.spec_scope_revision() == successor.spec_scope_revision(),
            transition.spec_scope_validity() == successor.spec_scope_validity(),
            transition.spec_scope_use_limit() == successor.spec_scope_use_limit(),
            transition.spec_previous_remaining_uses()
                == previous_remaining.spec_remaining(),
            transition.spec_successor_remaining_uses() == successor.spec_remaining_uses(),
            transition.spec_successor_time_epoch() == successor.spec_time_epoch(),
            transition.spec_successor_greatest_tick() == successor.spec_greatest_tick(),
            transition.spec_successor_issued_at() == successor.spec_issued_at(),
            transition.spec_successor_issuance_digest() == successor.spec_issuance_digest(),
            transition.spec_successor_issuance_command_id()
                == successor.spec_issuance_command_id(),
    {
        let (
            action_id,
            action_digest,
            permission,
            _,
            _,
            _,
            _,
            used_at,
        ) = request.into_parts();
        Self {
            action_id,
            action_digest,
            permission,
            used_at,
            transition_digest,
            previous_remaining,
            successor,
        }
    }

    /// Returns the exact action identifier bytes used by specifications.
    pub closed spec fn spec_action_id(&self) -> [u8; 16] { self.action_id.spec_bytes() }

    /// Returns the exact action digest bytes used by specifications.
    pub closed spec fn spec_action_digest(&self) -> [u8; 32] {
        self.action_digest.spec_bytes()
    }

    /// Returns the exact permission resource bytes used by specifications.
    pub closed spec fn spec_permission_resource_id(&self) -> [u8; 16] {
        self.permission.spec_resource_id()
    }

    /// Returns the exact permission capability name used by specifications.
    pub closed spec fn spec_permission_capability_name(&self) -> Seq<u8> {
        self.permission.spec_capability_name()
    }

    /// Returns the exact accepted use instant used by specifications.
    pub closed spec fn spec_used_at(&self) -> AuthorityInstant { self.used_at }

    /// Returns the exact caller-supplied logical transition digest.
    pub closed spec fn spec_transition_digest(&self) -> [u8; 32] {
        self.transition_digest.spec_bytes()
    }

    /// Returns the preserved actor identity used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.successor.spec_scope_actor_id()
    }

    /// Returns the preserved stable role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> ActorRole {
        self.successor.spec_scope_role()
    }

    /// Returns the preserved environment identity used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.successor.spec_scope_environment_id()
    }

    /// Returns the preserved ordered permissions used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<Permission> {
        self.successor.spec_scope_permissions()
    }

    /// Returns the preserved revision tuple used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> RevisionTuple {
        self.successor.spec_scope_revision()
    }

    /// Returns the preserved validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.successor.spec_scope_validity()
    }

    /// Returns the exact capability-scope use limit carried by the successor.
    pub closed spec fn spec_scope_use_limit(&self) -> Option<int> {
        self.successor.spec_scope_use_limit()
    }

    /// Returns the exact use count immediately before this logical consumption.
    pub closed spec fn spec_previous_remaining_uses(&self) -> Option<int> {
        self.previous_remaining.spec_remaining()
    }

    /// Returns the exact successor remaining use bound used by specifications.
    pub closed spec fn spec_successor_remaining_uses(&self) -> Option<int> {
        self.successor.spec_remaining_uses()
    }

    /// Returns the accepted successor time floor used by specifications.
    pub closed spec fn spec_successor_time_epoch(&self) -> int {
        self.successor.spec_time_epoch()
    }

    /// Returns the accepted successor greatest tick used by specifications.
    pub closed spec fn spec_successor_greatest_tick(&self) -> int {
        self.successor.spec_greatest_tick()
    }

    /// Returns the preserved issuance instant in the successor capability.
    pub closed spec fn spec_successor_issued_at(&self) -> AuthorityInstant {
        self.successor.spec_issued_at()
    }

    /// Returns the preserved issuance digest in the successor capability.
    pub closed spec fn spec_successor_issuance_digest(&self) -> [u8; 32] {
        self.successor.spec_issuance_digest()
    }

    /// Returns the preserved issuance command in the successor capability.
    pub closed spec fn spec_successor_issuance_command_id(&self) -> [u8; 16] {
        self.successor.spec_issuance_command_id()
    }

    /// Returns the exact action identity.
    #[must_use]
    pub const fn action_id(&self) -> (action_id: ActionId)
        ensures action_id.spec_bytes() == self.spec_action_id(),
    { self.action_id }

    /// Returns the exact action digest.
    #[must_use]
    pub const fn action_digest(&self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_action_digest(),
    { self.action_digest }

    /// Returns the exact capability permission consumed for the action.
    #[must_use]
    pub const fn permission(&self) -> (permission: &Permission)
        ensures
            permission.spec_resource_id() == self.spec_permission_resource_id(),
            permission.spec_capability_name() == self.spec_permission_capability_name(),
    { &self.permission }

    /// Returns the complete exact capability scope preserved by the transition.
    #[must_use]
    pub const fn scope(&self) -> (scope: &CapabilityScope)
        ensures
            scope.spec_actor_id() == self.spec_scope_actor_id(),
            scope.spec_role() == self.spec_scope_role(),
            scope.spec_environment_id() == self.spec_scope_environment_id(),
            scope.spec_permissions() == self.spec_scope_permissions(),
            scope.spec_revision() == self.spec_scope_revision(),
            scope.spec_validity() == self.spec_scope_validity(),
            scope.spec_use_limit().spec_remaining() == self.spec_scope_use_limit(),
    { self.successor.scope() }

    /// Returns the accepted authority-time observation for this use.
    #[must_use]
    pub const fn used_at(&self) -> (used_at: AuthorityInstant)
        ensures used_at == self.spec_used_at(),
    { self.used_at }

    /// Returns the exact transition digest supplied by the transition planner.
    #[must_use]
    pub const fn transition_digest(&self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_transition_digest(),
    { self.transition_digest }

    /// Returns the use bound before this one successful use.
    #[must_use]
    pub const fn previous_remaining(&self) -> (remaining: UseLimit)
        ensures remaining.spec_remaining() == self.spec_previous_remaining_uses(),
    { self.previous_remaining }

    /// Borrows the move-only successor capability.
    #[must_use]
    pub const fn successor(&self) -> (successor: &Capability)
        ensures
            successor.spec_remaining_uses() == self.spec_successor_remaining_uses(),
            successor.spec_time_epoch() == self.spec_successor_time_epoch(),
            successor.spec_greatest_tick() == self.spec_successor_greatest_tick(),
            successor.spec_issued_at() == self.spec_successor_issued_at(),
            successor.spec_issuance_digest() == self.spec_successor_issuance_digest(),
            successor.spec_issuance_command_id()
                == self.spec_successor_issuance_command_id(),
    { &self.successor }

    /// Consumes the transition and returns the move-only successor capability.
    #[must_use]
    pub fn into_successor(self) -> Capability { self.successor }
}

} // verus!
