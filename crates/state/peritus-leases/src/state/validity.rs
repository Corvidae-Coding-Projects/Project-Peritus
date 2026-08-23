//! Exact aggregate-validity predicates and their executable refinements.

use super::{ActiveLease, LeaseAggregate};
use crate::{LeaseError, ReconciliationCorrelation};
use peritus_types::Generation;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn active_state_is_valid(
    aggregate: &LeaseAggregate,
    active: ActiveLease,
) -> bool {
    active.issued_at.spec_epoch() == aggregate.authority_time.spec_epoch()
        && active.expires_at.spec_epoch() == aggregate.authority_time.spec_epoch()
        && active.issued_at.spec_tick_millis() < active.expires_at.spec_tick_millis()
        && aggregate.authority_time.spec_greatest_tick_millis()
            >= active.issued_at.spec_tick_millis()
        && aggregate.authority_time.spec_greatest_tick_millis()
            < active.expires_at.spec_tick_millis()
        && aggregate.version.spec_value() < u64::MAX as int
}

pub(super) const fn active_state_is_valid_runtime(
    aggregate: &LeaseAggregate,
    active: ActiveLease,
) -> (valid: bool)
    ensures valid == active_state_is_valid(aggregate, active),
{
    active.issued_at.epoch().get() == aggregate.authority_time.epoch().get()
        && active.expires_at.epoch().get() == aggregate.authority_time.epoch().get()
        && active.issued_at.tick_millis() < active.expires_at.tick_millis()
        && aggregate.authority_time.greatest_tick_millis() >= active.issued_at.tick_millis()
        && aggregate.authority_time.greatest_tick_millis() < active.expires_at.tick_millis()
        && aggregate.version.get() < u64::MAX
}

pub(crate) open spec fn correlation_is_valid(
    aggregate: &LeaseAggregate,
    correlation: ReconciliationCorrelation,
) -> bool {
    crate::model::concrete::identity::exact_lease_scope_match(
        correlation.spec_scope(),
        aggregate.scope,
    )
        && correlation.spec_fenced_generation().spec_value() + 1
            == aggregate.generation.spec_value()
}

const fn generations_are_successive(
    fenced: Generation,
    current: Generation,
) -> (matches: bool)
    ensures matches == (fenced.spec_value() + 1 == current.spec_value()),
{
    let fenced_value = fenced.get();
    let current_value = current.get();
    if fenced_value == u64::MAX {
        false
    } else {
        fenced_value + 1 == current_value
    }
}

pub(super) const fn correlation_is_valid_runtime(
    aggregate: &LeaseAggregate,
    correlation: ReconciliationCorrelation,
) -> (valid: bool)
    ensures valid == correlation_is_valid(aggregate, correlation),
{
    let scope_matches = crate::model::concrete::identity::lease_scopes_equal(
        correlation.scope(),
        aggregate.scope,
    );
    let generation_matches = generations_are_successive(
        correlation.fenced_generation(),
        aggregate.generation,
    );
    let valid = scope_matches && generation_matches;
    assert(valid == correlation_is_valid(aggregate, correlation));
    valid
}

pub(super) const fn validity_result(valid: bool) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => valid,
            Err(error) => !valid && error == LeaseError::CorruptState,
        },
{
    if valid {
        Ok(())
    } else {
        Err(LeaseError::CorruptState)
    }
}

} // verus!
