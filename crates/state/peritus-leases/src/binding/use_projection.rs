//! Complete owned projection of a move-only policy-use command.

use super::LeasePermissionBinding;
use crate::{LeaseClaim, UseLease};
use peritus_policy::{ActorRole, AuthorityInstant, UseLimit, ValidityWindow};
use peritus_types::{
    ActionId, ActorId, CommandId, EnvironmentId, Generation, RevisionTuple, Sha256Digest,
};
use vstd::prelude::*;

verus! {

/// Complete owned projection of the move-only policy-use input relevant to a lease transition.
///
/// This projection carries no capability and grants no authority. It exists so C0 can compare the
/// exact accepted logical command and its full policy scope with the durable transaction record.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct LeaseUseCommandBinding {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) action_id: ActionId,
    pub(crate) action_digest: Sha256Digest,
    pub(crate) permission: LeasePermissionBinding,
    pub(crate) actor_id: ActorId,
    pub(crate) role: ActorRole,
    pub(crate) environment_id: EnvironmentId,
    pub(crate) scope_permissions: Vec<LeasePermissionBinding>,
    pub(crate) revision: RevisionTuple,
    pub(crate) validity: ValidityWindow,
    pub(crate) scope_use_limit: UseLimit,
    pub(crate) used_at: AuthorityInstant,
    pub(crate) transition_digest: Sha256Digest,
    pub(crate) previous_remaining: UseLimit,
    pub(crate) successor_remaining: UseLimit,
    pub(crate) successor_time_epoch: Generation,
    pub(crate) successor_greatest_tick_millis: u64,
    pub(crate) successor_issued_at: AuthorityInstant,
    pub(crate) successor_issuance_digest: Sha256Digest,
    pub(crate) successor_issuance_command_id: CommandId,
}

impl LeaseUseCommandBinding {
    pub(crate) fn from_command(command: &UseLease) -> (binding: Self)
        ensures binding.matches_command(command),
    {
        let capability = &command.capability_use;
        let scope = capability.scope();
        let source_permissions = scope.permissions().as_slice();
        let mut scope_permissions: Vec<LeasePermissionBinding> = Vec::new();
        let mut index = 0_usize;
        while index < source_permissions.len()
            invariant
                0 <= index <= source_permissions.len(),
                scope_permissions@.len() == index,
                source_permissions@ == capability.spec_scope_permissions(),
                forall |prior: int| 0 <= prior < index ==>
                    scope_permissions@[prior].matches_permission(&source_permissions@[prior]),
            decreases source_permissions.len() - index,
        {
            scope_permissions.push(LeasePermissionBinding::from_permission(
                &source_permissions[index],
            ));
            index += 1;
        }
        proof {
            assert(scope_permissions@.len() == source_permissions@.len());
            assert forall |permission_index: int|
                0 <= permission_index < source_permissions@.len() implies
                    scope_permissions@[permission_index]
                        .matches_permission(&source_permissions@[permission_index]) by {
            }
            assert(source_permissions@ == capability.spec_scope_permissions());
        }
        let successor = capability.successor();
        let binding = Self {
            command_id: command.command_id,
            claim: command.claim,
            observed_at: command.observed_at,
            action_id: capability.action_id(),
            action_digest: capability.action_digest(),
            permission: LeasePermissionBinding::from_permission(capability.permission()),
            actor_id: scope.actor_id(),
            role: scope.role(),
            environment_id: scope.environment_id(),
            scope_permissions,
            revision: scope.revision(),
            validity: scope.validity(),
            scope_use_limit: scope.use_limit(),
            used_at: capability.used_at(),
            transition_digest: capability.transition_digest(),
            previous_remaining: capability.previous_remaining(),
            successor_remaining: successor.remaining_uses(),
            successor_time_epoch: successor.time_state().epoch(),
            successor_greatest_tick_millis: successor.time_state().greatest_tick_millis(),
            successor_issued_at: successor.issued_at(),
            successor_issuance_digest: successor.issuance_digest(),
            successor_issuance_command_id: successor.issuance_command_id(),
        };
        proof {
            assert(binding.matches_scope_permissions(
                command.capability_use.spec_scope_permissions(),
            ));
            assert(binding.permission.resource_id.spec_bytes()
                == command.capability_use.spec_permission_resource_id());
            assert(binding.permission.capability_name.spec_bytes()
                == command.capability_use.spec_permission_capability_name());
            assert(binding.scope_use_limit.spec_remaining()
                == command.capability_use.spec_scope_use_limit());
            assert(binding.previous_remaining.spec_remaining()
                == command.capability_use.spec_previous_remaining_uses());
            assert(binding.command_id == command.command_id);
            assert(binding.claim == command.claim);
            assert(binding.observed_at == command.observed_at);
            assert(binding.action_id.spec_bytes()
                == command.capability_use.spec_action_id());
            assert(binding.action_digest.spec_bytes()
                == command.capability_use.spec_action_digest());
            assert(binding.actor_id.spec_bytes()
                == command.capability_use.spec_scope_actor_id());
            assert(binding.role == command.capability_use.spec_scope_role());
            assert(binding.environment_id.spec_bytes()
                == command.capability_use.spec_scope_environment_id());
            assert(binding.revision == command.capability_use.spec_scope_revision());
            assert(binding.validity == command.capability_use.spec_scope_validity());
            assert(binding.used_at == command.capability_use.spec_used_at());
            assert(binding.transition_digest.spec_bytes()
                == command.capability_use.spec_transition_digest());
            assert(binding.successor_remaining.spec_remaining()
                == command.capability_use.spec_successor_remaining_uses());
            assert(binding.successor_time_epoch.spec_value()
                == command.capability_use.spec_successor_time_epoch());
            assert(binding.successor_greatest_tick_millis as int
                == command.capability_use.spec_successor_greatest_tick());
            assert(binding.successor_issued_at
                == command.capability_use.spec_successor_issued_at());
            assert(binding.successor_issuance_digest.spec_bytes()
                == command.capability_use.spec_successor_issuance_digest());
            assert(binding.successor_issuance_command_id.spec_bytes()
                == command.capability_use.spec_successor_issuance_command_id());
            assert(binding.matches_command(command));
        }
        binding
    }

    pub(crate) open spec fn matches_scope_permissions(
        &self,
        permissions: Seq<peritus_policy::Permission>,
    ) -> bool {
        self.scope_permissions@.len() == permissions.len()
            && forall |index: int| 0 <= index < permissions.len() ==>
                self.scope_permissions@[index].matches_permission(&permissions[index])
    }

    pub(crate) open spec fn matches_command(&self, command: &UseLease) -> bool {
        self.matches_parts(
            command.command_id,
            command.claim,
            command.observed_at,
            &command.capability_use,
        )
    }

    pub(crate) open spec fn matches_parts(
        &self,
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        capability_use: &peritus_policy::CapabilityUseTransition,
    ) -> bool {
        self.matches_lease_inputs(command_id, claim, observed_at)
            && self.matches_capability_use(capability_use)
    }

    pub(crate) open spec fn matches_lease_inputs(
        &self,
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
    ) -> bool {
        self.command_id == command_id
            && self.claim == claim
            && self.observed_at.spec_epoch() == observed_at.spec_epoch()
            && self.observed_at.spec_tick_millis() == observed_at.spec_tick_millis()
    }

    pub(crate) open spec fn matches_capability_use(
        &self,
        capability_use: &peritus_policy::CapabilityUseTransition,
    ) -> bool {
        self.action_id.spec_bytes() == capability_use.spec_action_id()
            && self.action_digest.spec_bytes() == capability_use.spec_action_digest()
            && self.permission.resource_id.spec_bytes()
                == capability_use.spec_permission_resource_id()
            && self.permission.capability_name.spec_bytes()
                == capability_use.spec_permission_capability_name()
            && self.actor_id.spec_bytes() == capability_use.spec_scope_actor_id()
            && self.role == capability_use.spec_scope_role()
            && self.environment_id.spec_bytes()
                == capability_use.spec_scope_environment_id()
            && self.matches_scope_permissions(capability_use.spec_scope_permissions())
            && self.revision == capability_use.spec_scope_revision()
            && self.validity == capability_use.spec_scope_validity()
            && self.scope_use_limit.spec_remaining()
                == capability_use.spec_scope_use_limit()
            && self.used_at == capability_use.spec_used_at()
            && self.transition_digest.spec_bytes()
                == capability_use.spec_transition_digest()
            && self.previous_remaining.spec_remaining()
                == capability_use.spec_previous_remaining_uses()
            && self.successor_remaining.spec_remaining()
                == capability_use.spec_successor_remaining_uses()
            && self.successor_time_epoch.spec_value()
                == capability_use.spec_successor_time_epoch()
            && self.successor_greatest_tick_millis as int
                == capability_use.spec_successor_greatest_tick()
            && self.successor_issued_at == capability_use.spec_successor_issued_at()
            && self.successor_issuance_digest.spec_bytes()
                == capability_use.spec_successor_issuance_digest()
            && self.successor_issuance_command_id.spec_bytes()
                == capability_use.spec_successor_issuance_command_id()
    }

    /// Returns the exact lease command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }
    /// Returns the exact current lease claim.
    #[must_use]
    pub const fn claim(&self) -> LeaseClaim { self.claim }
    /// Returns the exact lease authority-time observation.
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant { self.observed_at }
    /// Returns the exact action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.action_id }
    /// Returns the exact action digest.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest { self.action_digest }
    /// Borrows the exact consumed permission.
    #[must_use]
    pub const fn permission(&self) -> &LeasePermissionBinding { &self.permission }
    /// Returns the exact capability actor.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId { self.actor_id }
    /// Returns the exact capability actor role.
    #[must_use]
    pub const fn role(&self) -> ActorRole { self.role }
    /// Returns the exact capability environment.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId { self.environment_id }
    /// Borrows the complete exact canonical capability-scope permission sequence.
    #[must_use]
    pub const fn scope_permissions(&self) -> &[LeasePermissionBinding] {
        self.scope_permissions.as_slice()
    }
    /// Returns the exact capability revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple { self.revision }
    /// Returns the exact capability validity window.
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow { self.validity }
    /// Returns the exact capability-scope use bound.
    #[must_use]
    pub const fn scope_use_limit(&self) -> UseLimit { self.scope_use_limit }
    /// Returns the exact capability-use instant.
    #[must_use]
    pub const fn used_at(&self) -> AuthorityInstant { self.used_at }
    /// Returns the exact policy logical transition digest.
    #[must_use]
    pub const fn transition_digest(&self) -> Sha256Digest { self.transition_digest }
    /// Returns the exact use count before consumption.
    #[must_use]
    pub const fn previous_remaining(&self) -> UseLimit { self.previous_remaining }
    /// Returns the exact use count in the successor capability.
    #[must_use]
    pub const fn successor_remaining(&self) -> UseLimit { self.successor_remaining }
    /// Returns the exact successor capability time epoch.
    #[must_use]
    pub const fn successor_time_epoch(&self) -> Generation { self.successor_time_epoch }
    /// Returns the exact successor capability greatest authority tick.
    #[must_use]
    pub const fn successor_greatest_tick_millis(&self) -> u64 {
        self.successor_greatest_tick_millis
    }
    /// Returns the exact preserved successor issuance instant.
    #[must_use]
    pub const fn successor_issued_at(&self) -> AuthorityInstant { self.successor_issued_at }
    /// Returns the exact preserved successor issuance digest.
    #[must_use]
    pub const fn successor_issuance_digest(&self) -> Sha256Digest {
        self.successor_issuance_digest
    }
    /// Returns the exact preserved successor issuance command identity.
    #[must_use]
    pub const fn successor_issuance_command_id(&self) -> CommandId {
        self.successor_issuance_command_id
    }
}

} // verus!
