//! Exact executable policy/lease intersection validation.

use crate::model::concrete::identity::identifier_values_equal;
use crate::{LeaseAggregate, LeaseError, PolicyIntersectionDimension, UseLease};

pub(super) const fn validate_policy_intersection(
    aggregate: &LeaseAggregate,
    command: &UseLease,
) -> Result<(), LeaseError> {
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
    if !identifier_values_equal(policy_actor_bytes, claim_actor_bytes) {
        return mismatch(PolicyIntersectionDimension::Actor);
    }
    let policy_environment = scope.environment_id();
    let policy_environment_bytes = *policy_environment.as_bytes();
    let lease_environment_bytes = *aggregate.scope.environment.as_bytes();
    if !identifier_values_equal(policy_environment_bytes, lease_environment_bytes) {
        return mismatch(PolicyIntersectionDimension::Environment);
    }
    let revision = scope.revision();
    let policy_workspace = revision.workspace_id();
    let policy_workspace_bytes = *policy_workspace.as_bytes();
    let lease_workspace_bytes = *aggregate.scope.workspace.as_bytes();
    if !identifier_values_equal(policy_workspace_bytes, lease_workspace_bytes) {
        return mismatch(PolicyIntersectionDimension::Workspace);
    }
    if revision.workspace_generation().get() != aggregate.generation.get() {
        return mismatch(PolicyIntersectionDimension::Generation);
    }
    let policy_resource = policy.permission().resource_id();
    let policy_resource_bytes = *policy_resource.as_bytes();
    let lease_resource_bytes = *aggregate.scope.resource.as_bytes();
    if !identifier_values_equal(policy_resource_bytes, lease_resource_bytes) {
        return mismatch(PolicyIntersectionDimension::ResourcePermission);
    }
    if scope.validity().expires_at().epoch().get() != aggregate.authority_time.epoch().get() {
        return mismatch(PolicyIntersectionDimension::ClockEpoch);
    }
    match scope.validity().contains(observed_at) {
        Ok(true) => Ok(()),
        Ok(false) | Err(_) => Err(LeaseError::PolicyUseInvalid),
    }
}
const fn mismatch(dimension: PolicyIntersectionDimension) -> Result<(), LeaseError> {
    Err(LeaseError::PolicyIntersectionMismatch(dimension))
}
