//! Exact insertion positions for live and historical dispatch identities.

#[cfg(verus_only)]
use crate::SchedulerState;
use crate::{DispatchId, SchedulerReservation};
use vstd::prelude::*;

verus! {

pub(super) fn reservation_slot(values: &[SchedulerReservation], id: DispatchId) -> (at: usize)
    ensures
        at <= values@.len(),
        SchedulerState::reservation_records_ordered(values@) ==> (
            (forall |index: int| 0 <= index < at ==>
                values@[index].spec_dispatch_id().spec_precedes(&id))
            && (forall |index: int| at <= index < values@.len() ==>
                !values@[index].spec_dispatch_id().spec_precedes(&id))
        ),
{
    let mut size = values.len();
    if size == 0 {
        return 0;
    }
    let mut base: usize = 0;
    while size > 1
        invariant
            0 < size <= values.len(),
            base < values.len(),
            base + size <= values.len(),
            SchedulerState::reservation_records_ordered(values@) ==> (
                (forall |index: int| 0 <= index < base ==>
                    values@[index].spec_dispatch_id().spec_precedes(&id))
                && (forall |index: int| base + size <= index < values@.len() ==>
                    !values@[index].spec_dispatch_id().spec_precedes(&id))
            ),
        decreases size,
    {
        let half = size / 2;
        let mid = base + half;
        let observed = values[mid].dispatch_id();
        let ghost old_base = base;
        let ghost old_size = size;
        if !id.precedes(&observed) {
            base = mid;
        }
        size -= half;
        proof {
            if SchedulerState::reservation_records_ordered(values@) {
                assert forall |index: int| 0 <= index < base implies
                    values@[index].spec_dispatch_id().spec_precedes(&id) by {
                    if old_base <= index {
                        let prior = values@[index].spec_dispatch_id();
                        assert(index < mid);
                        assert(prior.spec_precedes(&observed));
                        DispatchId::order_total(&id, &observed);
                        if observed != id {
                            DispatchId::order_transitive(&prior, &observed, &id);
                        }
                    }
                }
                assert forall |index: int| base + size <= index < values@.len() implies
                    !values@[index].spec_dispatch_id().spec_precedes(&id) by {
                    if index < old_base + old_size {
                        let later = values@[index].spec_dispatch_id();
                        assert(mid <= index);
                        assert(id.spec_precedes(&observed));
                        if mid < index {
                            assert(observed.spec_precedes(&later));
                            DispatchId::order_transitive(&id, &observed, &later);
                        }
                        DispatchId::order_asymmetric(&id, &later);
                    }
                }
            }
        };
    }
    let observed = values[base].dispatch_id();
    if observed.precedes(&id) { base + 1 } else { base }
}


pub(super) fn dispatch_slot(values: &[DispatchId], id: DispatchId) -> (at: usize)
    ensures
        at <= values@.len(),
        SchedulerState::dispatch_ids_ordered(values@) ==> (
            (forall |index: int| 0 <= index < at ==> !id.spec_precedes(&values@[index]))
            && (forall |index: int| at <= index < values@.len() ==>
                id.spec_precedes(&values@[index]))
        ),
{
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            forall |prior: int| 0 <= prior < index ==> !id.spec_precedes(&values@[prior]),
        decreases values@.len() - index,
    {
        if id.precedes(&values[index]) {
            proof {
                assert forall |later: int| SchedulerState::dispatch_ids_ordered(values@)
                    && index <= later < values@.len() implies
                        id.spec_precedes(&values@[later]) by {
                    if index < later {
                        assert(values@[index as int].spec_precedes(&values@[later]));
                        DispatchId::order_transitive(&id, &values@[index as int], &values@[later]);
                    }
                }
            }
            return index;
        }
        index += 1;
    }
    index
}

} // verus!
