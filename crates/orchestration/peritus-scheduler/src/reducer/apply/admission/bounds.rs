//! Queue and retention bounds for the actual admitted record insertion.

use super::contracts::{admitted_record, preflight_rejection};
use crate::state::queue;
use crate::{SchedulerReservation, SchedulerSemantics, SchedulerState, WorkRecord, WorkSpec};
use vstd::prelude::*;

verus! {

proof fn insertion_count<A>(values: Seq<A>, value: A, at: int, predicate: spec_fn(A) -> bool)
    requires 0 <= at <= values.len(),
    ensures values.insert(at, value).filter(predicate).len()
        == values.filter(predicate).len() + if predicate(value) { 1int } else { 0int },
    decreases values.len(),
{
    if at == values.len() {
        assert(values.insert(at, value) =~= values.push(value));
        values.lemma_filter_len_push(predicate, value);
    } else {
        let prefix = values.drop_last();
        insertion_count(prefix, value, at, predicate);
        prefix.lemma_filter_len_push(predicate, values.last());
        prefix.insert(at, value).lemma_filter_len_push(predicate, values.last());
        assert(prefix.push(values.last()) =~= values);
        assert(prefix.insert(at, value).push(values.last()) =~= values.insert(at, value));
    }
}

/// Distinct active work needs distinct reservation positions, even without canonical ordering.
proof fn active_work_count(work: Seq<WorkRecord>, reservations: Seq<SchedulerReservation>)
    requires
        crate::verified::work_identities_unique(work),
        crate::verified::active_work_has_reservations(work, reservations),
    ensures work.filter(|record: WorkRecord| queue::is_active(record)).len() <= reservations.len(),
    decreases work.len(),
{
    reveal(queue::is_active);
    reveal(crate::verified::work_phase_retains_reservation);
    reveal(crate::verified::work_identities_unique);
    reveal(crate::verified::active_work_has_reservations);
    reveal(crate::verified::active_work_has_reservation);
    if work.len() == 0 {
        reveal(Seq::filter);
        return;
    }
    let prefix = work.drop_last();
    assert(crate::verified::work_identities_unique(prefix));
    let active = |record: WorkRecord| queue::is_active(record);
    prefix.lemma_filter_len_push(active, work.last());
    assert(prefix.push(work.last()) =~= work);
    if !active(work.last()) {
        assert(crate::verified::active_work_has_reservations(prefix, reservations));
        active_work_count(prefix, reservations);
    } else {
        let removed = choose |index: int| #![trigger reservations[index]]
            0 <= index < reservations.len()
                && reservations[index].spec_work_id() == work.last().spec_definition().spec_id();
        let remaining = reservations.remove(removed);
        assert forall |work_index: int| #![trigger prefix[work_index]]
            0 <= work_index < prefix.len() implies
                crate::verified::active_work_has_reservation(prefix[work_index], remaining) by {
            if active(prefix[work_index]) {
                let source = choose |index: int| #![trigger reservations[index]]
                    0 <= index < reservations.len()
                        && reservations[index].spec_work_id()
                            == prefix[work_index].spec_definition().spec_id();
                assert(source != removed) by {
                    assert(work[work_index].spec_definition().spec_id()
                        != work[work.len() - 1].spec_definition().spec_id());
                }
                let target = if source < removed { source } else { source - 1 };
                assert(0 <= target < remaining.len());
                assert(remaining[target] == reservations[source]);
                assert(exists |index: int| #![trigger remaining[index]]
                    0 <= index < remaining.len()
                        && remaining[index].spec_work_id()
                            == prefix[work_index].spec_definition().spec_id());
            }
        }
        crate::verified::active_work_has_reservations_intro(prefix, remaining);
        active_work_count(prefix, remaining);
    }
}

proof fn waiting_active_partition(work: Seq<WorkRecord>)
    ensures work.filter(|record: WorkRecord|
        queue::is_waiting(record) || queue::is_active(record)).len()
        == work.filter(|record: WorkRecord| queue::is_waiting(record)).len()
            + work.filter(|record: WorkRecord| queue::is_active(record)).len(),
    decreases work.len(),
{
    let waiting = |record: WorkRecord| queue::is_waiting(record);
    let active = |record: WorkRecord| queue::is_active(record);
    let combined = |record: WorkRecord| queue::is_waiting(record) || queue::is_active(record);
    if work.len() == 0 {
        reveal(Seq::filter);
    } else {
        let prefix = work.drop_last();
        waiting_active_partition(prefix);
        prefix.lemma_filter_len_push(waiting, work.last());
        prefix.lemma_filter_len_push(active, work.last());
        prefix.lemma_filter_len_push(combined, work.last());
        assert(prefix.push(work.last()) =~= work);
        reveal(queue::is_waiting);
        reveal(queue::is_active);
        assert(!(waiting(work.last()) && active(work.last())));
        if waiting(work.last()) {
            assert(combined(work.last()) && !active(work.last()));
        } else if active(work.last()) {
            assert(combined(work.last()));
        } else {
            assert(!combined(work.last()));
        }
    }
}

/// The admitted waiting item consumes exactly one slot under either historical semantics.
pub(super) proof fn admission_preserves_bounds(
    before: &SchedulerState,
    after: &SchedulerState,
    spec: &WorkSpec,
    record: WorkRecord,
    at: int,
)
    requires
        preflight_rejection(before, spec).is_none(),
        after.spec_binding() == before.spec_binding(),
        0 <= at <= before.spec_work().len(),
        after.spec_work() == before.spec_work().insert(at, record),
        admitted_record(record, spec, after.spec_enqueue_ordinal()),
    ensures
        after.spec_work().len() <= after.spec_binding().spec_limits().spec_retained_work(),
        queue::admission_pressure(after) == queue::admission_pressure(before) + 1,
        queue::admission_pressure(after) <= after.spec_binding().spec_limits().spec_queued_work(),
        before.spec_reservation_reducer_ready() ==> queue::queue_bound(after),
{
    reveal(preflight_rejection);
    reveal(admitted_record);
    reveal(queue::is_waiting);
    reveal(queue::can_return);
    reveal(queue::is_active);
    let waiting = |record: WorkRecord| queue::is_waiting(record);
    let strict = |record: WorkRecord| queue::is_waiting(record) || queue::can_return(record);
    let active = |record: WorkRecord| queue::is_active(record);
    assert(waiting(record));
    assert(!active(record));
    insertion_count(before.spec_work(), record, at, waiting);
    insertion_count(before.spec_work(), record, at, strict);
    insertion_count(before.spec_work(), record, at, active);
    reveal(queue::admission_pressure);
    reveal(queue::queue_bound);
    if before.spec_binding().spec_semantics() == SchedulerSemantics::LegacyQueueV1
        && before.spec_reservation_reducer_ready()
    {
        reveal(SchedulerState::spec_reservation_reducer_ready);
        reveal(SchedulerState::spec_reservation_invariant);
        reveal(crate::verified::reservation_invariant_parts);
        active_work_count(before.spec_work(), before.spec_reservations());
        waiting_active_partition(after.spec_work());
    }
}

} // verus!
