//! Exact policy-use and lease-use intersection lemmas.

#[cfg(verus_only)]
use crate::{model, LeaseUseTransition};
use vstd::prelude::*;

verus! {

pub(crate) proof fn exact_intersection_is_no_broader_than_both_inputs(
    lease_actor: int,
    policy_actor: int,
    lease_environment: int,
    policy_environment: int,
    lease_workspace: int,
    policy_workspace: int,
    lease_generation: int,
    policy_generation: int,
    lease_resource: int,
    policy_resource: int,
)
    requires model::exact_authority_intersection(
        lease_actor,
        policy_actor,
        lease_environment,
        policy_environment,
        lease_workspace,
        policy_workspace,
        lease_generation,
        policy_generation,
        lease_resource,
        policy_resource,
    ),
    ensures
        lease_actor == policy_actor,
        lease_environment == policy_environment,
        lease_workspace == policy_workspace,
        lease_generation == policy_generation,
        lease_resource == policy_resource,
{
}

pub(crate) proof fn stale_policy_generation_cannot_intersect(
    lease_generation: int,
    policy_generation: int,
)
    requires lease_generation != policy_generation,
    ensures !model::exact_authority_intersection(
        1,
        1,
        1,
        1,
        1,
        1,
        lease_generation,
        policy_generation,
        1,
        1,
    ),
{
}

pub(crate) proof fn executable_logical_use_remains_bound_to_current_claim(
    logical_use: &LeaseUseTransition,
)
    requires model::concrete_lease_use_is_current(logical_use),
    ensures
        model::concrete_claim_is_current(
            &logical_use.lease.next,
            logical_use.claim,
        ),
{
    model::concrete::authority::lease_use_implies_current_claim(logical_use);
}

} // verus!
