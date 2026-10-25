//! Exact aggregate-validity predicates and their executable refinements.

use super::{ActiveLease, LeaseAggregate};
use crate::{LeaseError, ReconciliationCorrelation};
use peritus_types::Generation;

pub(super) const fn active_state_is_valid_runtime(
    aggregate: &LeaseAggregate,
    active: ActiveLease,
) -> bool {
    active.issued_at.epoch().get() == aggregate.authority_time.epoch().get()
        && active.expires_at.epoch().get() == aggregate.authority_time.epoch().get()
        && active.issued_at.tick_millis() < active.expires_at.tick_millis()
        && aggregate.authority_time.greatest_tick_millis() >= active.issued_at.tick_millis()
        && aggregate.authority_time.greatest_tick_millis() < active.expires_at.tick_millis()
        && aggregate.version.get() < u64::MAX
}
const fn generations_are_successive(fenced: Generation, current: Generation) -> bool {
    let fenced_value = fenced.get();
    let current_value = current.get();
    if fenced_value == u64::MAX { false } else { fenced_value + 1 == current_value }
}
pub(super) const fn correlation_is_valid_runtime(
    aggregate: &LeaseAggregate,
    correlation: ReconciliationCorrelation,
) -> bool {
    let scope_matches =
        crate::model::concrete::identity::lease_scopes_equal(correlation.scope(), aggregate.scope);
    let generation_matches =
        generations_are_successive(correlation.fenced_generation(), aggregate.generation);

    scope_matches && generation_matches
}
pub(super) const fn validity_result(valid: bool) -> Result<(), LeaseError> {
    if valid { Ok(()) } else { Err(LeaseError::CorruptState) }
}
