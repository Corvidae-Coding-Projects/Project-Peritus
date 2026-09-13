//! Exact canonical insertion positions from the production binary-search algorithm.

#[cfg(verus_only)]
use crate::SchedulerState;
use crate::{WorkId, WorkRecord, WorkerId, WorkerRecord};
use vstd::prelude::*;

verus! {

pub(super) fn worker_slot(values: &[WorkerRecord], id: WorkerId) -> (at: usize)
    ensures
        at <= values@.len(),
        SchedulerState::worker_records_ordered(values@) ==> (
            (forall |index: int| 0 <= index < at ==>
                values@[index].spec_descriptor().spec_id().spec_precedes(&id))
            && (forall |index: int| at <= index < values@.len() ==>
                !values@[index].spec_descriptor().spec_id().spec_precedes(&id))
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
            SchedulerState::worker_records_ordered(values@) ==> (
                (forall |index: int| 0 <= index < base ==>
                    values@[index].spec_descriptor().spec_id().spec_precedes(&id))
                && (forall |index: int| base + size <= index < values@.len() ==>
                    !values@[index].spec_descriptor().spec_id().spec_precedes(&id))
            ),
        decreases size,
    {
        let half = size / 2;
        let mid = base + half;
        let observed = values[mid].descriptor().id();
        let ghost old_base = base;
        let ghost old_size = size;
        if !id.precedes(&observed) {
            base = mid;
        }
        size -= half;
        proof {
            if SchedulerState::worker_records_ordered(values@) {
                assert forall |index: int| 0 <= index < base implies
                    values@[index].spec_descriptor().spec_id().spec_precedes(&id) by {
                    if old_base <= index {
                        let prior = values@[index].spec_descriptor().spec_id();
                        assert(index < mid);
                        assert(prior.spec_precedes(&observed));
                        WorkerId::order_total(&id, &observed);
                        if observed != id {
                            WorkerId::order_transitive(&prior, &observed, &id);
                        }
                    }
                }
                assert forall |index: int| base + size <= index < values@.len() implies
                    !values@[index].spec_descriptor().spec_id().spec_precedes(&id) by {
                    if index < old_base + old_size {
                        let later = values@[index].spec_descriptor().spec_id();
                        assert(mid <= index);
                        assert(id.spec_precedes(&observed));
                        if mid < index {
                            assert(observed.spec_precedes(&later));
                            WorkerId::order_transitive(&id, &observed, &later);
                        }
                        WorkerId::order_asymmetric(&id, &later);
                    }
                }
            }
        };
    }
    let observed = values[base].descriptor().id();
    if observed.precedes(&id) { base + 1 } else { base }
}

pub(super) fn work_slot(values: &[WorkRecord], id: WorkId) -> (at: usize)
    ensures
        at <= values@.len(),
        SchedulerState::work_records_ordered(values@) ==> (
            (forall |index: int| 0 <= index < at ==>
                values@[index].spec_definition().spec_id().spec_precedes(&id))
            && (forall |index: int| at <= index < values@.len() ==>
                !values@[index].spec_definition().spec_id().spec_precedes(&id))
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
            SchedulerState::work_records_ordered(values@) ==> (
                (forall |index: int| 0 <= index < base ==>
                    values@[index].spec_definition().spec_id().spec_precedes(&id))
                && (forall |index: int| base + size <= index < values@.len() ==>
                    !values@[index].spec_definition().spec_id().spec_precedes(&id))
            ),
        decreases size,
    {
        let half = size / 2;
        let mid = base + half;
        let observed = values[mid].spec().id();
        let ghost old_base = base;
        let ghost old_size = size;
        if !id.precedes(&observed) {
            base = mid;
        }
        size -= half;
        proof {
            if SchedulerState::work_records_ordered(values@) {
                assert forall |index: int| 0 <= index < base implies
                    values@[index].spec_definition().spec_id().spec_precedes(&id) by {
                    if old_base <= index {
                        let prior = values@[index].spec_definition().spec_id();
                        assert(index < mid);
                        assert(prior.spec_precedes(&observed));
                        WorkId::order_total(&id, &observed);
                        if observed != id {
                            WorkId::order_transitive(&prior, &observed, &id);
                        }
                    }
                }
                assert forall |index: int| base + size <= index < values@.len() implies
                    !values@[index].spec_definition().spec_id().spec_precedes(&id) by {
                    if index < old_base + old_size {
                        let later = values@[index].spec_definition().spec_id();
                        assert(mid <= index);
                        assert(id.spec_precedes(&observed));
                        if mid < index {
                            assert(observed.spec_precedes(&later));
                            WorkId::order_transitive(&id, &observed, &later);
                        }
                        WorkId::order_asymmetric(&id, &later);
                    }
                }
            }
        };
    }
    let observed = values[base].spec().id();
    if observed.precedes(&id) { base + 1 } else { base }
}

} // verus!
