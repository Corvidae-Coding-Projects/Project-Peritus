//! Exact move-only lease/policy-use output oracle.

use super::{ReferenceState, record::assert_record_fields};
use peritus_leases::{
    LeaseClaim, LeaseCommandBindingKind, LeaseTransitionKind, LeaseUseCommandBinding,
    LeaseUseTransition,
};
use peritus_policy::{
    ActorRole, AuthorityInstant, CapabilityUseTransition, Permission, UseLimit, ValidityWindow,
};
use peritus_types::{
    ActionId, ActorId, CommandId, EnvironmentId, Generation, ResourceId, RevisionTuple,
    Sha256Digest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct PermissionSnapshot {
    resource_id: ResourceId,
    capability_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapabilitySnapshot {
    action_id: ActionId,
    action_digest: Sha256Digest,
    permission: PermissionSnapshot,
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
    scope_permissions: Vec<PermissionSnapshot>,
    revision: RevisionTuple,
    validity: ValidityWindow,
    scope_use_limit: UseLimit,
    used_at: AuthorityInstant,
    transition_digest: Sha256Digest,
    previous_remaining: UseLimit,
    successor_remaining: UseLimit,
    successor_time_epoch: Generation,
    successor_greatest_tick_millis: u64,
    successor_issued_at: AuthorityInstant,
    successor_issuance_digest: Sha256Digest,
    successor_issuance_command_id: CommandId,
}

/// Exact expected ownership and field projection for one accepted use.
pub struct ExpectedUseOutput {
    command_id: CommandId,
    claim: LeaseClaim,
    observed_at: AuthorityInstant,
    effective_expires_at: AuthorityInstant,
    capability: CapabilitySnapshot,
}

impl ExpectedUseOutput {
    pub fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        capability: &CapabilityUseTransition,
    ) -> Self {
        let capability = CapabilitySnapshot::from(capability);
        let lease_expiry = claim.expires_at();
        let capability_expiry = capability.validity.expires_at();
        let effective_expires_at = if lease_expiry.tick_millis() <= capability_expiry.tick_millis()
        {
            lease_expiry
        } else {
            capability_expiry
        };
        Self { command_id, claim, observed_at, effective_expires_at, capability }
    }

    pub fn assert_matches(
        &self,
        logical_use: &LeaseUseTransition,
        before: ReferenceState,
        after: ReferenceState,
        seed: u8,
        step: &str,
    ) {
        assert_eq!(logical_use.claim(), self.claim, "seed {seed} {step}: claim");
        assert_eq!(
            logical_use.action_id(),
            self.capability.action_id,
            "seed {seed} {step}: action id"
        );
        assert_eq!(
            logical_use.action_digest(),
            self.capability.action_digest,
            "seed {seed} {step}: action digest"
        );
        assert_eq!(
            logical_use.effective_expires_at(),
            self.effective_expires_at,
            "seed {seed} {step}: effective expiry"
        );
        assert_eq!(
            CapabilitySnapshot::from(logical_use.capability_use()),
            self.capability,
            "seed {seed} {step}: returned capability transition"
        );
        let kind = LeaseTransitionKind::Used {
            action_id: self.capability.action_id,
            action_digest: self.capability.action_digest,
        };
        assert_record_fields(
            logical_use.lease_transition(),
            Some(before),
            after,
            self.command_id,
            kind,
            seed,
            step,
        );
        let binding = logical_use.lease_transition().record().binding();
        assert_eq!(binding.kind(), LeaseCommandBindingKind::Use, "seed {seed} {step}: family");
        self.assert_binding(binding.as_use().expect("expected use binding"), seed, step);
        after.assert_refines(logical_use.lease_transition().next(), seed, step);
    }

    pub fn assert_rejected_command(
        &self,
        command: &peritus_leases::UseLease,
        seed: u8,
        step: &str,
    ) {
        assert_eq!(command.command_id(), self.command_id, "seed {seed} {step}: command id");
        assert_eq!(command.claim(), self.claim, "seed {seed} {step}: claim");
        assert_eq!(command.observed_at(), self.observed_at, "seed {seed} {step}: observation");
        assert_eq!(
            CapabilitySnapshot::from(command.capability_use()),
            self.capability,
            "seed {seed} {step}: capability transition"
        );
    }

    fn assert_binding(&self, actual: &LeaseUseCommandBinding, seed: u8, step: &str) {
        let context = format!("seed {seed} {step}: use binding");
        assert_eq!(actual.command_id(), self.command_id, "{context}: command id");
        assert_eq!(actual.claim(), self.claim, "{context}: claim");
        assert_eq!(actual.observed_at(), self.observed_at, "{context}: observation");
        assert_eq!(actual.action_id(), self.capability.action_id, "{context}: action id");
        assert_eq!(actual.action_digest(), self.capability.action_digest, "{context}: digest");
        assert_eq!(
            actual.permission().resource_id(),
            self.capability.permission.resource_id,
            "{context}: permission resource"
        );
        assert_eq!(
            actual.permission().capability_name().as_str(),
            self.capability.permission.capability_name,
            "{context}: permission name"
        );
        assert_eq!(actual.actor_id(), self.capability.actor_id, "{context}: actor");
        assert_eq!(actual.role(), self.capability.role, "{context}: role");
        assert_eq!(
            actual.environment_id(),
            self.capability.environment_id,
            "{context}: environment"
        );
        assert_permissions(actual, &self.capability.scope_permissions, &context);
        assert_eq!(actual.revision(), self.capability.revision, "{context}: revision");
        assert_eq!(actual.validity(), self.capability.validity, "{context}: validity");
        assert_eq!(
            actual.scope_use_limit(),
            self.capability.scope_use_limit,
            "{context}: scope uses"
        );
        assert_eq!(actual.used_at(), self.capability.used_at, "{context}: used at");
        assert_eq!(
            actual.transition_digest(),
            self.capability.transition_digest,
            "{context}: transition digest"
        );
        assert_eq!(
            actual.previous_remaining(),
            self.capability.previous_remaining,
            "{context}: prior uses"
        );
        assert_eq!(
            actual.successor_remaining(),
            self.capability.successor_remaining,
            "{context}: successor uses"
        );
        assert_eq!(
            actual.successor_time_epoch(),
            self.capability.successor_time_epoch,
            "{context}: successor epoch"
        );
        assert_eq!(
            actual.successor_greatest_tick_millis(),
            self.capability.successor_greatest_tick_millis,
            "{context}: successor tick"
        );
        assert_eq!(
            actual.successor_issued_at(),
            self.capability.successor_issued_at,
            "{context}: successor issue time"
        );
        assert_eq!(
            actual.successor_issuance_digest(),
            self.capability.successor_issuance_digest,
            "{context}: successor issue digest"
        );
        assert_eq!(
            actual.successor_issuance_command_id(),
            self.capability.successor_issuance_command_id,
            "{context}: successor issue command"
        );
    }
}

impl CapabilitySnapshot {
    fn from(value: &CapabilityUseTransition) -> Self {
        let scope = value.scope();
        let successor = value.successor();
        Self {
            action_id: value.action_id(),
            action_digest: value.action_digest(),
            permission: PermissionSnapshot::from(value.permission()),
            actor_id: scope.actor_id(),
            role: scope.role(),
            environment_id: scope.environment_id(),
            scope_permissions: scope
                .permissions()
                .as_slice()
                .iter()
                .map(PermissionSnapshot::from)
                .collect(),
            revision: scope.revision(),
            validity: scope.validity(),
            scope_use_limit: scope.use_limit(),
            used_at: value.used_at(),
            transition_digest: value.transition_digest(),
            previous_remaining: value.previous_remaining(),
            successor_remaining: successor.remaining_uses(),
            successor_time_epoch: successor.time_state().epoch(),
            successor_greatest_tick_millis: successor.time_state().greatest_tick_millis(),
            successor_issued_at: successor.issued_at(),
            successor_issuance_digest: successor.issuance_digest(),
            successor_issuance_command_id: successor.issuance_command_id(),
        }
    }
}

fn assert_permissions(
    actual: &LeaseUseCommandBinding,
    expected: &[PermissionSnapshot],
    context: &str,
) {
    assert_eq!(actual.scope_permissions().len(), expected.len(), "{context}: permissions length");
    for (index, (actual, expected)) in
        actual.scope_permissions().iter().zip(expected.iter()).enumerate()
    {
        assert_eq!(
            actual.resource_id(),
            expected.resource_id,
            "{context}: permission {index} resource"
        );
        assert_eq!(
            actual.capability_name().as_str(),
            expected.capability_name,
            "{context}: permission {index} name"
        );
    }
}

impl PermissionSnapshot {
    fn from(value: &Permission) -> Self {
        Self {
            resource_id: value.resource_id(),
            capability_name: value.capability_name().as_str().to_owned(),
        }
    }
}
