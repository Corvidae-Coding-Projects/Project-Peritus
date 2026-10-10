//! Exact equality for complete policy-use command projections.

use super::super::aggregate::claims_equal;

use super::super::{bytes16_equal, bytes32_equal};

use crate::{LeasePermissionBinding, LeaseUseCommandBinding};
use peritus_policy::{ActorRole, AuthorityInstant, UseLimit, ValidityWindow};
use peritus_types::RevisionTuple;

use std::cmp::Ordering;

const fn instants_equal(left: AuthorityInstant, right: AuthorityInstant) -> bool {
    left.epoch().get() == right.epoch().get() && left.tick_millis() == right.tick_millis()
}
const fn roles_equal(left: ActorRole, right: ActorRole) -> bool {
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
fn revisions_equal(left: RevisionTuple, right: RevisionTuple) -> bool {
    bytes16_equal(*left.acceptance_spec_id().as_bytes(), *right.acceptance_spec_id().as_bytes())
        && bytes16_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
        && bytes16_equal(*left.workspace_id().as_bytes(), *right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes16_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && bytes16_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
}
const fn validities_equal(left: ValidityWindow, right: ValidityWindow) -> bool {
    instants_equal(left.not_before(), right.not_before())
        && instants_equal(left.expires_at(), right.expires_at())
}
const fn use_limits_equal(left: UseLimit, right: UseLimit) -> bool {
    match (left.remaining(), right.remaining()) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => left_value == right_value,
        _ => false,
    }
}
fn permissions_equal(left: &LeasePermissionBinding, right: &LeasePermissionBinding) -> bool {
    bytes16_equal(*left.resource_id.as_bytes(), *right.resource_id.as_bytes())
        && matches!(left.capability_name.canonical_cmp(&right.capability_name), Ordering::Equal,)
}
pub(super) fn use_bindings_equal(
    left: &LeaseUseCommandBinding,
    right: &LeaseUseCommandBinding,
) -> bool {
    let leading_fields_equal =
        bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
            && claims_equal(left.claim, right.claim)
            && instants_equal(left.observed_at, right.observed_at)
            && bytes16_equal(*left.action_id.as_bytes(), *right.action_id.as_bytes())
            && bytes32_equal(*left.action_digest.as_bytes(), *right.action_digest.as_bytes())
            && permissions_equal(&left.permission, &right.permission)
            && bytes16_equal(*left.actor_id.as_bytes(), *right.actor_id.as_bytes())
            && roles_equal(left.role, right.role)
            && bytes16_equal(*left.environment_id.as_bytes(), *right.environment_id.as_bytes());
    if !leading_fields_equal || left.scope_permissions.len() != right.scope_permissions.len() {
        return false;
    }
    let mut index = 0_usize;
    while index < left.scope_permissions.len() {
        if !permissions_equal(&left.scope_permissions[index], &right.scope_permissions[index]) {
            return false;
        }
        index += 1;
    }
    let trailing_fields_equal = revisions_equal(left.revision, right.revision)
        && validities_equal(left.validity, right.validity)
        && use_limits_equal(left.scope_use_limit, right.scope_use_limit)
        && instants_equal(left.used_at, right.used_at)
        && bytes32_equal(*left.transition_digest.as_bytes(), *right.transition_digest.as_bytes())
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
    trailing_fields_equal
}
