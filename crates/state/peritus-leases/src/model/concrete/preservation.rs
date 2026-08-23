//! Exact aggregate preservation for consuming reducer rejection paths.

use vstd::prelude::*;

verus! {

/// Concrete snapshot equality used by rejected-transition preservation obligations.
pub(crate) open spec fn concrete_snapshot_preserved(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
) -> bool {
    before.scope == after.scope
        && before.generation == after.generation
        && before.version == after.version
        && before.state == after.state
        && before.authority_time.spec_epoch() == after.authority_time.spec_epoch()
        && before.authority_time.spec_greatest_tick_millis()
            == after.authority_time.spec_greatest_tick_millis()
}

/// Public-contract wrapper proving a rejected consuming reducer returns the exact prior aggregate.
pub closed spec fn concrete_rejection_preserves_input(
    before: &crate::LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
) -> bool {
    concrete_snapshot_preserved(before, &failure.spec_aggregate())
}

/// Establishes the opaque rejection contract from immutable input identity.
pub(crate) proof fn establish_rejection_preservation(
    before: &crate::LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
)
    requires concrete_snapshot_preserved(before, &failure.spec_aggregate()),
    ensures concrete_rejection_preserves_input(before, failure),
{
}

/// Projects exact snapshot preservation from the opaque public rejection contract.
pub(crate) proof fn rejection_implies_snapshot_preserved(
    before: &crate::LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
)
    requires concrete_rejection_preserves_input(before, failure),
    ensures concrete_snapshot_preserved(before, &failure.spec_aggregate()),
{
}

/// Establishes structural preservation for the immutable rejected reducer input.
pub(crate) proof fn establish_snapshot_reflexivity(before: &crate::LeaseAggregate)
    ensures concrete_snapshot_preserved(before, before),
{
}

} // verus!
