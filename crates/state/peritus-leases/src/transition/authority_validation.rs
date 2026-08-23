//! Exact executable policy/lease intersection validation.

use crate::model::concrete::identity::identifier_values_equal;
use crate::{LeaseAggregate, LeaseError, PolicyIntersectionDimension, UseLease};
use vstd::prelude::*;

verus! {

pub(super) const fn validate_policy_intersection(
    aggregate: &LeaseAggregate,
    command: &UseLease,
) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => {
                crate::model::concrete::rejections::authority::policy_intersection_error(
                    aggregate,
                    command,
                ).is_none()
                    && crate::model::concrete::authority::concrete_policy_intersection(
                        aggregate,
                        command,
                    )
            }
            Err(error) => {
                crate::model::concrete::rejections::authority::policy_intersection_error(
                    aggregate,
                    command,
                ) == Some(error)
            }
        },
{
    proof { command.reveal_exact_fields(); }
    let policy = &command.capability_use;
    let scope = policy.scope();
    let used_at = policy.used_at();
    let observed_at = command.observed_at();
    if used_at.epoch().get() != observed_at.epoch().get()
        || used_at.tick_millis() != observed_at.tick_millis()
    {
        return mismatch(PolicyIntersectionDimension::ClockEpoch);
    }
    let policy_actor = scope.actor_id();
    let claim_actor = command.claim.holder.actor_id;
    let policy_actor_bytes = *policy_actor.as_bytes();
    let claim_actor_bytes = *claim_actor.as_bytes();
    assert(policy_actor_bytes == policy.spec_scope_actor_id());
    assert(claim_actor_bytes == command.claim.holder.actor_id.spec_bytes());
    if !identifier_values_equal(policy_actor_bytes, claim_actor_bytes) {
        return mismatch(PolicyIntersectionDimension::Actor);
    }
    let policy_environment = scope.environment_id();
    let policy_environment_bytes = *policy_environment.as_bytes();
    let lease_environment_bytes = *aggregate.scope.environment.as_bytes();
    assert(policy_environment_bytes == policy.spec_scope_environment_id());
    assert(lease_environment_bytes == aggregate.scope.environment.spec_bytes());
    if !identifier_values_equal(policy_environment_bytes, lease_environment_bytes) {
        return mismatch(PolicyIntersectionDimension::Environment);
    }
    let revision = scope.revision();
    let policy_workspace = revision.workspace_id();
    let policy_workspace_bytes = *policy_workspace.as_bytes();
    let lease_workspace_bytes = *aggregate.scope.workspace.as_bytes();
    assert(policy_workspace_bytes
        == policy.spec_scope_revision().spec_workspace_id().spec_bytes());
    assert(lease_workspace_bytes == aggregate.scope.workspace.spec_bytes());
    if !identifier_values_equal(policy_workspace_bytes, lease_workspace_bytes) {
        return mismatch(PolicyIntersectionDimension::Workspace);
    }
    if revision.workspace_generation().get() != aggregate.generation.get() {
        return mismatch(PolicyIntersectionDimension::Generation);
    }
    let policy_resource = policy.permission().resource_id();
    let policy_resource_bytes = *policy_resource.as_bytes();
    let lease_resource_bytes = *aggregate.scope.resource.as_bytes();
    assert(policy_resource_bytes == policy.spec_permission_resource_id());
    assert(lease_resource_bytes == aggregate.scope.resource.spec_bytes());
    if !identifier_values_equal(policy_resource_bytes, lease_resource_bytes) {
        return mismatch(PolicyIntersectionDimension::ResourcePermission);
    }
    if scope.validity().expires_at().epoch().get() != aggregate.authority_time.epoch().get() {
        return mismatch(PolicyIntersectionDimension::ClockEpoch);
    }
    match scope.validity().contains(observed_at) {
        Ok(true) => {
            assert(crate::model::concrete_instant_matches(
                policy.spec_used_at(),
                command.observed_at,
            ));
            assert(crate::model::concrete_identifier_matches(
                policy.spec_scope_actor_id(),
                command.claim.holder.actor_id.spec_bytes(),
            ));
            assert(crate::model::concrete_identifier_matches(
                policy.spec_scope_environment_id(),
                aggregate.scope.environment.spec_bytes(),
            ));
            assert(crate::model::concrete_identifier_matches(
                policy.spec_scope_revision().spec_workspace_id().spec_bytes(),
                aggregate.scope.workspace.spec_bytes(),
            ));
            assert(policy.spec_scope_revision().spec_workspace_generation().spec_value()
                == aggregate.generation.spec_value());
            assert(crate::model::concrete_identifier_matches(
                policy.spec_permission_resource_id(),
                aggregate.scope.resource.spec_bytes(),
            ));
            assert(policy.spec_scope_validity().spec_contains(command.observed_at));
            assert(crate::model::concrete::authority::concrete_policy_intersection(
                aggregate,
                command,
            ));
            Ok(())
        }
        Ok(false) | Err(_) => Err(LeaseError::PolicyUseInvalid),
    }
}

const fn mismatch(
    dimension: PolicyIntersectionDimension,
) -> (result: Result<(), LeaseError>)
    ensures result == Err(LeaseError::PolicyIntersectionMismatch(dimension)),
{
    Err(LeaseError::PolicyIntersectionMismatch(dimension))
}

} // verus!
