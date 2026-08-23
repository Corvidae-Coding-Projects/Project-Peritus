//! Refinement lemmas for executable versions and authority-time floors.

#[cfg(verus_only)]
use crate::{model, LeaseAggregate, LeaseTransitionRecord};
use vstd::prelude::*;

verus! {

pub(crate) proof fn checked_successor_advances_version_once(before: int)
    ensures model::version_advances_once(before, before + 1),
{
}

pub(crate) proof fn accepted_same_epoch_time_never_regresses(
    epoch: int,
    previous_tick: int,
    candidate_tick: int,
)
    requires previous_tick <= candidate_tick,
    ensures model::time_floor_accepts(epoch, previous_tick, epoch, candidate_tick),
{
}

pub(crate) proof fn epoch_change_is_not_an_ordinary_time_observation(
    previous_epoch: int,
    candidate_epoch: int,
    previous_tick: int,
    candidate_tick: int,
)
    requires previous_epoch != candidate_epoch,
    ensures !model::time_floor_accepts(
        previous_epoch,
        previous_tick,
        candidate_epoch,
        candidate_tick,
    ),
{
}

pub(crate) proof fn typed_record_refines_concrete_successor(
    before: &LeaseAggregate,
    after: &LeaseAggregate,
    record: LeaseTransitionRecord,
)
    requires model::concrete_record_matches(before, after, record),
    ensures
        record.scope == before.scope,
        record.scope == after.scope,
        record.before_generation == Some(before.generation),
        record.after_generation == after.generation,
        record.before_version == Some(before.version),
        record.after_version == after.version,
{
    model::concrete::project_concrete_record(before, after, record);
}

pub(crate) proof fn immutable_reducer_input_is_preserved_on_rejection(before: &LeaseAggregate)
    ensures model::concrete_snapshot_preserved(before, before),
{
    model::concrete::establish_snapshot_reflexivity(before);
}

} // verus!
