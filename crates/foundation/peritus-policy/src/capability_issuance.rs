//! Move-only logical capability issuance transitions.

use crate::{Capability, CapabilityIssuancePlan};
#[cfg(verus_only)]
use crate::{ActorRole, AuthorityInstant, Permission};
use peritus_types::{CommandId, Sha256Digest};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Logical issuance transition to be independently validated and durably committed by later layers.
///
/// Its fields are intentionally private outside this crate:
///
/// ```compile_fail
/// let _forged = peritus_policy::CapabilityIssuanceTransition {
///     command_id: loop {},
///     transition_digest: loop {},
///     capability: loop {},
/// };
/// ```
///
/// A successful issuance transition cannot be cloned or implicitly copied:
///
/// ```compile_fail
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<peritus_policy::CapabilityIssuanceTransition>();
/// ```
///
/// ```compile_fail
/// fn requires_copy<T: Copy>() {}
/// requires_copy::<peritus_policy::CapabilityIssuanceTransition>();
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityIssuanceTransition {
    command_id: CommandId,
    transition_digest: Sha256Digest,
    capability: Capability,
}

impl CapabilityIssuanceTransition {
    /// Returns the exact issuance command identifier bytes used by specifications.
    pub closed spec fn spec_command_id(&self) -> [u8; 16] { self.command_id.spec_bytes() }

    /// Returns the exact logical transition digest bytes used by specifications.
    pub closed spec fn spec_transition_digest(&self) -> [u8; 32] {
        self.transition_digest.spec_bytes()
    }

    /// Returns the exact issued actor identity used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.capability.spec_scope_actor_id()
    }

    /// Returns the exact issued stable role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> ActorRole {
        self.capability.spec_scope_role()
    }

    /// Returns the exact issued environment identity used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.capability.spec_scope_environment_id()
    }

    /// Returns the exact issued permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<Permission> {
        self.capability.spec_scope_permissions()
    }

    /// Returns the exact issued revision tuple used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> RevisionTuple {
        self.capability.spec_scope_revision()
    }

    /// Returns the exact issued validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.capability.spec_scope_validity()
    }

    /// Returns the exact issued use limit used by specifications.
    pub closed spec fn spec_remaining_uses(&self) -> Option<int> {
        self.capability.spec_remaining_uses()
    }

    /// Returns the exact issuance instant used by specifications.
    pub closed spec fn spec_issued_at(&self) -> AuthorityInstant {
        self.capability.spec_issued_at()
    }

    /// Returns the command identity stored inside the issued capability.
    pub closed spec fn spec_capability_issuance_command_id(&self) -> [u8; 16] {
        self.capability.spec_issuance_command_id()
    }

    /// Returns the transition digest stored inside the issued capability.
    pub closed spec fn spec_capability_issuance_digest(&self) -> [u8; 32] {
        self.capability.spec_issuance_digest()
    }

    /// Returns the successor authority-time epoch stored inside the capability.
    pub closed spec fn spec_capability_time_epoch(&self) -> int {
        self.capability.spec_time_epoch()
    }

    /// Returns the successor greatest authority tick stored inside the capability.
    pub closed spec fn spec_capability_greatest_tick(&self) -> int {
        self.capability.spec_greatest_tick()
    }

    /// Returns the exact unique command identity bound to this issuance attempt.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }

    /// Returns the exact transition digest supplied by the transition planner.
    #[must_use]
    pub const fn transition_digest(&self) -> Sha256Digest { self.transition_digest }

    /// Borrows the logically issued capability without making it clonable.
    #[must_use]
    pub const fn capability(&self) -> &Capability { &self.capability }

    /// Consumes the transition and returns its move-only logical capability.
    #[must_use]
    pub fn into_capability(self) -> Capability { self.capability }
}

impl CapabilityIssuancePlan {
    /// Constructs a move-only logical issuance transition bound to an exact transition digest.
    ///
    /// The returned value is not proof of persistence and is not an effect permit.
    #[must_use]
    pub fn issue(
        self,
        command_id: CommandId,
        transition_digest: Sha256Digest,
    ) -> (transition: CapabilityIssuanceTransition)
        ensures
            transition.spec_command_id() == command_id.spec_bytes(),
            transition.spec_transition_digest() == transition_digest.spec_bytes(),
            transition.spec_capability_issuance_command_id() == command_id.spec_bytes(),
            transition.spec_capability_issuance_digest() == transition_digest.spec_bytes(),
            transition.spec_scope_actor_id() == self.spec_scope_actor_id(),
            transition.spec_scope_role() == self.spec_scope_role(),
            transition.spec_scope_environment_id() == self.spec_scope_environment_id(),
            transition.spec_scope_permissions() == self.spec_scope_permissions(),
            transition.spec_scope_revision() == self.spec_scope_revision(),
            transition.spec_scope_validity() == self.spec_scope_validity(),
            transition.spec_remaining_uses() == self.spec_scope_use_limit().spec_remaining(),
            transition.spec_issued_at() == self.spec_evaluated_at(),
            transition.spec_capability_time_epoch() == self.spec_time_epoch(),
            transition.spec_capability_greatest_tick() == self.spec_greatest_tick(),
    {
        let (scope, issued_at, time_state) = self.into_parts();
        CapabilityIssuanceTransition {
            command_id,
            transition_digest,
            capability: Capability::from_issuance(
                scope,
                issued_at,
                transition_digest,
                command_id,
                time_state,
            ),
        }
    }
}

} // verus!
