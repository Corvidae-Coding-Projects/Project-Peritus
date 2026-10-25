//! Complete owned projection of a move-only policy-use command.

use super::LeasePermissionBinding;
use crate::{LeaseClaim, UseLease};
use peritus_policy::{ActorRole, AuthorityInstant, UseLimit, ValidityWindow};
use peritus_types::{
    ActionId, ActorId, CommandId, EnvironmentId, Generation, RevisionTuple, Sha256Digest,
};

#[doc = " Complete owned projection of the move-only policy-use input relevant to a lease transition."]
#[doc = ""]
#[doc = " This projection carries no capability and grants no authority. It exists so C0 can compare the"]
#[doc = " exact accepted logical command and its full policy scope with the durable transaction record."]
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
    pub(crate) fn from_command(command: &UseLease) -> Self {
        let capability = &command.capability_use;
        let scope = capability.scope();
        let source_permissions = scope.permissions().as_slice();
        let mut scope_permissions: Vec<LeasePermissionBinding> = Vec::new();
        let mut index = 0_usize;
        while index < source_permissions.len() {
            scope_permissions
                .push(LeasePermissionBinding::from_permission(&source_permissions[index]));
            index += 1;
        }
        let successor = capability.successor();

        Self {
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
        }
    }
    #[doc = " Returns the exact lease command identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns the exact current lease claim."]
    #[must_use]
    pub const fn claim(&self) -> LeaseClaim {
        self.claim
    }
    #[doc = " Returns the exact lease authority-time observation."]
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
    #[doc = " Returns the exact action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    #[doc = " Returns the exact action digest."]
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        self.action_digest
    }
    #[doc = " Borrows the exact consumed permission."]
    #[must_use]
    pub const fn permission(&self) -> &LeasePermissionBinding {
        &self.permission
    }
    #[doc = " Returns the exact capability actor."]
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    #[doc = " Returns the exact capability actor role."]
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    #[doc = " Returns the exact capability environment."]
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    #[doc = " Borrows the complete exact canonical capability-scope permission sequence."]
    #[must_use]
    pub const fn scope_permissions(&self) -> &[LeasePermissionBinding] {
        self.scope_permissions.as_slice()
    }
    #[doc = " Returns the exact capability revision tuple."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the exact capability validity window."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    #[doc = " Returns the exact capability-scope use bound."]
    #[must_use]
    pub const fn scope_use_limit(&self) -> UseLimit {
        self.scope_use_limit
    }
    #[doc = " Returns the exact capability-use instant."]
    #[must_use]
    pub const fn used_at(&self) -> AuthorityInstant {
        self.used_at
    }
    #[doc = " Returns the exact policy logical transition digest."]
    #[must_use]
    pub const fn transition_digest(&self) -> Sha256Digest {
        self.transition_digest
    }
    #[doc = " Returns the exact use count before consumption."]
    #[must_use]
    pub const fn previous_remaining(&self) -> UseLimit {
        self.previous_remaining
    }
    #[doc = " Returns the exact use count in the successor capability."]
    #[must_use]
    pub const fn successor_remaining(&self) -> UseLimit {
        self.successor_remaining
    }
    #[doc = " Returns the exact successor capability time epoch."]
    #[must_use]
    pub const fn successor_time_epoch(&self) -> Generation {
        self.successor_time_epoch
    }
    #[doc = " Returns the exact successor capability greatest authority tick."]
    #[must_use]
    pub const fn successor_greatest_tick_millis(&self) -> u64 {
        self.successor_greatest_tick_millis
    }
    #[doc = " Returns the exact preserved successor issuance instant."]
    #[must_use]
    pub const fn successor_issued_at(&self) -> AuthorityInstant {
        self.successor_issued_at
    }
    #[doc = " Returns the exact preserved successor issuance digest."]
    #[must_use]
    pub const fn successor_issuance_digest(&self) -> Sha256Digest {
        self.successor_issuance_digest
    }
    #[doc = " Returns the exact preserved successor issuance command identity."]
    #[must_use]
    pub const fn successor_issuance_command_id(&self) -> CommandId {
        self.successor_issuance_command_id
    }
}
