//! Logical exclusive-holder lemmas for `INV-006`.

#[cfg(verus_only)]
use crate::{model, LeaseAggregate, LeaseClaim};
use vstd::prelude::*;

verus! {

pub(crate) proof fn two_current_claims_have_the_same_holder(
    current_generation: int,
    current_holder: int,
    first_generation: int,
    first_holder: int,
    second_generation: int,
    second_holder: int,
)
    requires
        model::logical_claim_is_current(
            current_generation,
            current_holder,
            first_generation,
            first_holder,
        ),
        model::logical_claim_is_current(
            current_generation,
            current_holder,
            second_generation,
            second_holder,
        ),
    ensures
        first_generation == second_generation,
        first_holder == second_holder,
{
}

pub(crate) proof fn active_representation_has_exactly_one_holder(
    phase: int,
    has_holder: bool,
)
    requires has_holder == (phase == 1),
    ensures
        phase == 1 ==> has_holder,
        phase != 1 ==> !has_holder,
{
}

pub(crate) proof fn executable_aggregate_has_one_current_holder(aggregate: &LeaseAggregate)
    ensures model::concrete_exclusive(aggregate),
{
    model::concrete::establish_concrete_exclusive(aggregate);
}

pub(crate) proof fn two_concrete_current_claims_are_identical_in_holder_and_generation(
    aggregate: &LeaseAggregate,
    first: LeaseClaim,
    second: LeaseClaim,
)
    requires
        model::concrete_claim_is_current(aggregate, first),
        model::concrete_claim_is_current(aggregate, second),
    ensures
        model::concrete_holder_matches(first.holder, second.holder),
        first.generation.spec_value() == second.generation.spec_value(),
        first.claim_version.spec_value() == second.claim_version.spec_value(),
{
    model::concrete::current_claims_match(aggregate, first, second);
}

} // verus!
