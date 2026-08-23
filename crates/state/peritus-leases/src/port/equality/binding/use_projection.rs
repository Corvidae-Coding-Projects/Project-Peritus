//! Exact equality for complete policy-use command projections.

use super::super::aggregate::claims_equal;
#[cfg(verus_only)]
use super::super::aggregate::claim_fields_match;
use super::super::{bytes16_equal, bytes32_equal};
#[cfg(verus_only)]
use super::super::{bytes16_match, bytes32_match};
use crate::{LeasePermissionBinding, LeaseUseCommandBinding};
use peritus_policy::{ActorRole, AuthorityInstant, UseLimit, ValidityWindow};
use peritus_types::RevisionTuple;
#[cfg(verus_only)]
use peritus_types::canonical_byte_order_from;
use std::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn instant_fields_match(left: AuthorityInstant, right: AuthorityInstant) -> bool {
    crate::model::concrete_instant_matches(left, right)
}

const fn instants_equal(left: AuthorityInstant, right: AuthorityInstant) -> (equal: bool)
    ensures equal == instant_fields_match(left, right),
{
    left.epoch().get() == right.epoch().get()
        && left.tick_millis() == right.tick_millis()
}

pub(crate) open spec fn role_fields_match(left: ActorRole, right: ActorRole) -> bool {
    left.spec_rank() == right.spec_rank()
}

const fn roles_equal(left: ActorRole, right: ActorRole) -> (equal: bool)
    ensures equal == role_fields_match(left, right),
{
    matches!(
        (left, right),
        (ActorRole::Writer, ActorRole::Writer)
            | (ActorRole::Fixer, ActorRole::Fixer)
            | (ActorRole::Reviewer, ActorRole::Reviewer)
            | (ActorRole::Evaluator, ActorRole::Evaluator)
            | (ActorRole::GateRunner, ActorRole::GateRunner)
            | (ActorRole::Orchestrator, ActorRole::Orchestrator)
            | (ActorRole::EvolutionAgent, ActorRole::EvolutionAgent)
            | (ActorRole::HumanAuthority, ActorRole::HumanAuthority)
            | (ActorRole::DaemonService, ActorRole::DaemonService)
            | (ActorRole::ProviderToolWorker, ActorRole::ProviderToolWorker)
            | (ActorRole::Plugin, ActorRole::Plugin)
    )
}

pub(crate) open spec fn revision_fields_match(left: RevisionTuple, right: RevisionTuple) -> bool {
    bytes16_match(
        left.spec_acceptance_spec_id().spec_bytes(),
        right.spec_acceptance_spec_id().spec_bytes(),
    ) && bytes16_match(
        left.spec_harness_id().spec_bytes(),
        right.spec_harness_id().spec_bytes(),
    ) && bytes16_match(
        left.spec_workspace_id().spec_bytes(),
        right.spec_workspace_id().spec_bytes(),
    ) && left.spec_workspace_generation().spec_value()
        == right.spec_workspace_generation().spec_value()
        && left.spec_workspace_revision().spec_value()
            == right.spec_workspace_revision().spec_value()
        && bytes16_match(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        )
        && bytes16_match(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        )
}

fn revisions_equal(left: RevisionTuple, right: RevisionTuple) -> (equal: bool)
    ensures equal == revision_fields_match(left, right),
{
    bytes16_equal(
        *left.acceptance_spec_id().as_bytes(),
        *right.acceptance_spec_id().as_bytes(),
    ) && bytes16_equal(
        *left.harness_id().as_bytes(),
        *right.harness_id().as_bytes(),
    ) && bytes16_equal(
        *left.workspace_id().as_bytes(),
        *right.workspace_id().as_bytes(),
    ) && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes16_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && bytes16_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
}

pub(crate) open spec fn validity_fields_match(left: ValidityWindow, right: ValidityWindow) -> bool {
    instant_fields_match(left.spec_not_before(), right.spec_not_before())
        && instant_fields_match(left.spec_expires_at(), right.spec_expires_at())
}

const fn validities_equal(left: ValidityWindow, right: ValidityWindow) -> (equal: bool)
    ensures equal == validity_fields_match(left, right),
{
    instants_equal(left.not_before(), right.not_before())
        && instants_equal(left.expires_at(), right.expires_at())
}

pub(crate) open spec fn use_limit_fields_match(left: UseLimit, right: UseLimit) -> bool {
    left.spec_remaining() == right.spec_remaining()
}

const fn use_limits_equal(left: UseLimit, right: UseLimit) -> (equal: bool)
    ensures equal == use_limit_fields_match(left, right),
{
    match (left.remaining(), right.remaining()) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => left_value == right_value,
        _ => false,
    }
}

pub(crate) open spec fn permission_fields_match(
    left: &LeasePermissionBinding,
    right: &LeasePermissionBinding,
) -> bool {
    bytes16_match(left.resource_id.spec_bytes(), right.resource_id.spec_bytes())
        && canonical_byte_order_from(
            left.capability_name.spec_bytes(),
            right.capability_name.spec_bytes(),
            0,
        ) == Ordering::Equal
}

fn permissions_equal(
    left: &LeasePermissionBinding,
    right: &LeasePermissionBinding,
) -> (equal: bool)
    ensures equal == permission_fields_match(left, right),
{
    bytes16_equal(*left.resource_id.as_bytes(), *right.resource_id.as_bytes())
        && matches!(
            left.capability_name.canonical_cmp(&right.capability_name),
            Ordering::Equal,
        )
}

pub(crate) open spec fn leading_use_fields_match(
    left: &LeaseUseCommandBinding,
    right: &LeaseUseCommandBinding,
) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && claim_fields_match(left.claim, right.claim)
        && instant_fields_match(left.observed_at, right.observed_at)
        && bytes16_match(left.action_id.spec_bytes(), right.action_id.spec_bytes())
        && bytes32_match(
            left.action_digest.spec_bytes(),
            right.action_digest.spec_bytes(),
        )
        && permission_fields_match(&left.permission, &right.permission)
        && bytes16_match(left.actor_id.spec_bytes(), right.actor_id.spec_bytes())
        && role_fields_match(left.role, right.role)
        && bytes16_match(
            left.environment_id.spec_bytes(),
            right.environment_id.spec_bytes(),
        )
}

pub(crate) open spec fn permission_sequence_fields_match(
    left: &LeaseUseCommandBinding,
    right: &LeaseUseCommandBinding,
) -> bool {
    left.scope_permissions@.len() == right.scope_permissions@.len()
        && forall |index: int| #![auto] 0 <= index < left.scope_permissions@.len() ==>
            permission_fields_match(
                &left.scope_permissions@[index],
                &right.scope_permissions@[index],
            )
}

pub(crate) open spec fn trailing_use_fields_match(
    left: &LeaseUseCommandBinding,
    right: &LeaseUseCommandBinding,
) -> bool {
    revision_fields_match(left.revision, right.revision)
        && validity_fields_match(left.validity, right.validity)
        && use_limit_fields_match(left.scope_use_limit, right.scope_use_limit)
        && instant_fields_match(left.used_at, right.used_at)
        && bytes32_match(
            left.transition_digest.spec_bytes(),
            right.transition_digest.spec_bytes(),
        )
        && use_limit_fields_match(left.previous_remaining, right.previous_remaining)
        && use_limit_fields_match(left.successor_remaining, right.successor_remaining)
        && left.successor_time_epoch.spec_value()
            == right.successor_time_epoch.spec_value()
        && left.successor_greatest_tick_millis == right.successor_greatest_tick_millis
        && instant_fields_match(left.successor_issued_at, right.successor_issued_at)
        && bytes32_match(
            left.successor_issuance_digest.spec_bytes(),
            right.successor_issuance_digest.spec_bytes(),
        )
        && bytes16_match(
            left.successor_issuance_command_id.spec_bytes(),
            right.successor_issuance_command_id.spec_bytes(),
        )
}

pub(crate) open spec fn use_binding_fields_match(
    left: &LeaseUseCommandBinding,
    right: &LeaseUseCommandBinding,
) -> bool {
    leading_use_fields_match(left, right)
        && permission_sequence_fields_match(left, right)
        && trailing_use_fields_match(left, right)
}

pub(super) fn use_bindings_equal(
    left: &LeaseUseCommandBinding,
    right: &LeaseUseCommandBinding,
) -> (equal: bool)
    ensures equal == use_binding_fields_match(left, right),
{
    let leading_fields_equal = bytes16_equal(
        *left.command_id.as_bytes(),
        *right.command_id.as_bytes(),
    )
        && claims_equal(left.claim, right.claim)
        && instants_equal(left.observed_at, right.observed_at)
        && bytes16_equal(*left.action_id.as_bytes(), *right.action_id.as_bytes())
        && bytes32_equal(*left.action_digest.as_bytes(), *right.action_digest.as_bytes())
        && permissions_equal(&left.permission, &right.permission)
        && bytes16_equal(*left.actor_id.as_bytes(), *right.actor_id.as_bytes())
        && roles_equal(left.role, right.role)
        && bytes16_equal(
            *left.environment_id.as_bytes(),
            *right.environment_id.as_bytes(),
        );
    proof {
        assert(leading_fields_equal == leading_use_fields_match(left, right));
    }
    if !leading_fields_equal || left.scope_permissions.len() != right.scope_permissions.len() {
        return false;
    }
    let mut index = 0_usize;
    while index < left.scope_permissions.len()
        invariant
            left.scope_permissions@.len() == right.scope_permissions@.len(),
            0 <= index <= left.scope_permissions.len(),
            forall |prior: int| 0 <= prior < index ==>
                permission_fields_match(
                    &left.scope_permissions@[prior],
                    &right.scope_permissions@[prior],
                ),
        decreases left.scope_permissions.len() - index,
    {
        if !permissions_equal(
            &left.scope_permissions[index],
            &right.scope_permissions[index],
        ) {
            return false;
        }
        index += 1;
    }
    proof {
        assert(left.scope_permissions@.len() == right.scope_permissions@.len());
        assert forall |permission_index: int|
            0 <= permission_index < left.scope_permissions@.len() implies
                permission_fields_match(
                    &left.scope_permissions@[permission_index],
                    &right.scope_permissions@[permission_index],
                ) by {
        }
        assert(permission_sequence_fields_match(left, right));
    }
    let trailing_fields_equal = revisions_equal(left.revision, right.revision)
        && validities_equal(left.validity, right.validity)
        && use_limits_equal(left.scope_use_limit, right.scope_use_limit)
        && instants_equal(left.used_at, right.used_at)
        && bytes32_equal(
            *left.transition_digest.as_bytes(),
            *right.transition_digest.as_bytes(),
        )
        && use_limits_equal(left.previous_remaining, right.previous_remaining)
        && use_limits_equal(left.successor_remaining, right.successor_remaining)
        && left.successor_time_epoch.get() == right.successor_time_epoch.get()
        && left.successor_greatest_tick_millis == right.successor_greatest_tick_millis
        && instants_equal(left.successor_issued_at, right.successor_issued_at)
        && bytes32_equal(
            *left.successor_issuance_digest.as_bytes(),
            *right.successor_issuance_digest.as_bytes(),
        )
        && bytes16_equal(
            *left.successor_issuance_command_id.as_bytes(),
            *right.successor_issuance_command_id.as_bytes(),
        );
    proof {
        assert(trailing_fields_equal == trailing_use_fields_match(left, right));
        assert(leading_use_fields_match(left, right));
        assert(trailing_fields_equal == use_binding_fields_match(left, right));
    }
    trailing_fields_equal
}

} // verus!
