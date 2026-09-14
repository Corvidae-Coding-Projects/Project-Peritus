//! Canonical identity ordering required by production binary searches and insertion.

use super::SchedulerState;
#[cfg(verus_only)]
use crate::{DispatchId, SchedulerReservation, WorkId, WorkRecord, WorkerId, WorkerRecord};
use vstd::prelude::*;

verus! {

impl SchedulerState {

    /// Strict canonical order for an arbitrary live-reservation sequence.
    pub open spec fn reservation_records_ordered(values: Seq<SchedulerReservation>) -> bool {
        forall |left: int, right: int| 0 <= left < right < values.len() ==>
            values[left].spec_dispatch_id().spec_precedes(&values[right].spec_dispatch_id())
    }

    /// Strict canonical order for an arbitrary historical-dispatch sequence.
    pub open spec fn dispatch_ids_ordered(values: Seq<DispatchId>) -> bool {
        forall |left: int, right: int| 0 <= left < right < values.len() ==>
            values[left].spec_precedes(&values[right])
    }

    /// Strict canonical order for an arbitrary sequence of retained workers.
    pub open spec fn worker_records_ordered(values: Seq<WorkerRecord>) -> bool {
        forall |left: int, right: int| 0 <= left < right < values.len() ==>
            values[left].spec_descriptor().spec_id().spec_precedes(
                &values[right].spec_descriptor().spec_id())
    }

    /// Strict canonical order for an arbitrary sequence of retained work.
    pub open spec fn work_records_ordered(values: Seq<WorkRecord>) -> bool {
        forall |left: int, right: int| 0 <= left < right < values.len() ==>
            values[left].spec_definition().spec_id().spec_precedes(
                &values[right].spec_definition().spec_id())
    }


    /// A fresh identity inserted at its lower bound preserves complete canonical order.
    pub(crate) proof fn worker_insertion_ordered(values: Seq<WorkerRecord>, value: WorkerRecord, at: int)
        requires
            0 <= at <= values.len(),
            Self::worker_records_ordered(values),
            crate::verified::worker_identity_absent(values, value.spec_descriptor().spec_id()),
            forall |index: int| 0 <= index < at ==>
                values[index].spec_descriptor().spec_id().spec_precedes(&value.spec_descriptor().spec_id()),
            forall |index: int| at <= index < values.len() ==>
                !values[index].spec_descriptor().spec_id().spec_precedes(&value.spec_descriptor().spec_id()),
        ensures Self::worker_records_ordered(values.insert(at, value)),
    {
        let next = values.insert(at, value);
        let id = value.spec_descriptor().spec_id();
        assert forall |left: int, right: int| 0 <= left < right < next.len() implies
            next[left].spec_descriptor().spec_id().spec_precedes(
                &next[right].spec_descriptor().spec_id()) by {
            if left == at {
                let later = values[right - 1].spec_descriptor().spec_id();
                assert(later != id);
                WorkerId::order_total(&id, &later);
            } else if right == at {
                assert(values[left].spec_descriptor().spec_id().spec_precedes(&id));
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < old_right < values.len());
                assert(values[old_left].spec_descriptor().spec_id().spec_precedes(
                    &values[old_right].spec_descriptor().spec_id()));
            }
        }
    }

    /// A fresh identity inserted at its lower bound preserves complete canonical order.
    pub(crate) proof fn work_insertion_ordered(values: Seq<WorkRecord>, value: WorkRecord, at: int)
        requires
            0 <= at <= values.len(),
            Self::work_records_ordered(values),
            crate::verified::work_identity_absent(values, value.spec_definition().spec_id()),
            forall |index: int| 0 <= index < at ==>
                values[index].spec_definition().spec_id().spec_precedes(&value.spec_definition().spec_id()),
            forall |index: int| at <= index < values.len() ==>
                !values[index].spec_definition().spec_id().spec_precedes(&value.spec_definition().spec_id()),
        ensures Self::work_records_ordered(values.insert(at, value)),
    {
        let next = values.insert(at, value);
        let id = value.spec_definition().spec_id();
        assert forall |left: int, right: int| 0 <= left < right < next.len() implies
            next[left].spec_definition().spec_id().spec_precedes(
                &next[right].spec_definition().spec_id()) by {
            if left == at {
                let later = values[right - 1].spec_definition().spec_id();
                assert(later != id);
                WorkId::order_total(&id, &later);
            } else if right == at {
                assert(values[left].spec_definition().spec_id().spec_precedes(&id));
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < old_right < values.len());
                assert(values[old_left].spec_definition().spec_id().spec_precedes(
                    &values[old_right].spec_definition().spec_id()));
            }
        }
    }

    /// A fresh identity inserted at its lower bound preserves complete canonical order.
    pub(crate) proof fn reservation_insertion_ordered(values: Seq<SchedulerReservation>, value: SchedulerReservation, at: int)
        requires
            0 <= at <= values.len(),
            Self::reservation_records_ordered(values),
            forall |index: int| 0 <= index < values.len() ==> values[index].spec_dispatch_id() != value.spec_dispatch_id(),
            forall |index: int| 0 <= index < at ==>
                values[index].spec_dispatch_id().spec_precedes(&value.spec_dispatch_id()),
            forall |index: int| at <= index < values.len() ==>
                !values[index].spec_dispatch_id().spec_precedes(&value.spec_dispatch_id()),
        ensures Self::reservation_records_ordered(values.insert(at, value)),
    {
        let next = values.insert(at, value);
        let id = value.spec_dispatch_id();
        assert forall |left: int, right: int| 0 <= left < right < next.len() implies
            next[left].spec_dispatch_id().spec_precedes(
                &next[right].spec_dispatch_id()) by {
            if left == at {
                let later = values[right - 1].spec_dispatch_id();
                assert(later != id);
                DispatchId::order_total(&id, &later);
            } else if right == at {
                assert(values[left].spec_dispatch_id().spec_precedes(&id));
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < old_right < values.len());
                assert(values[old_left].spec_dispatch_id().spec_precedes(
                    &values[old_right].spec_dispatch_id()));
            }
        }
    }

    /// A fresh historical identity inserted at its upper bound remains canonically ordered.
    pub(crate) proof fn dispatch_insertion_ordered(values: Seq<DispatchId>, id: DispatchId, at: int)
        requires
            0 <= at <= values.len(),
            Self::dispatch_ids_ordered(values),
            !values.contains(id),
            forall |index: int| 0 <= index < at ==> !id.spec_precedes(&values[index]),
            forall |index: int| at <= index < values.len() ==> id.spec_precedes(&values[index]),
        ensures Self::dispatch_ids_ordered(values.insert(at, id)),
    {
        let next = values.insert(at, id);
        assert forall |left: int, right: int| 0 <= left < right < next.len() implies
            next[left].spec_precedes(&next[right]) by {
            if left == at {
                assert(id.spec_precedes(&values[right - 1]));
            } else if right == at {
                assert(values[left] != id);
                DispatchId::order_total(&values[left], &id);
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < old_right < values.len());
                assert(values[old_left].spec_precedes(&values[old_right]));
            }
        }
    }

    /// Removing one retained reservation preserves the ordering of every remaining pair.
    pub(crate) proof fn reservation_removal_ordered(values: Seq<SchedulerReservation>, at: int)
        requires Self::reservation_records_ordered(values), 0 <= at < values.len(),
        ensures Self::reservation_records_ordered(values.remove(at)),
    {
        let next = values.remove(at);
        assert forall |left: int, right: int| 0 <= left < right < next.len() implies
            next[left].spec_dispatch_id().spec_precedes(&next[right].spec_dispatch_id()) by {
            let old_left = if left < at { left } else { left + 1 };
            let old_right = if right < at { right } else { right + 1 };
            assert(0 <= old_left < old_right < values.len());
            assert(values[old_left].spec_dispatch_id().spec_precedes(
                &values[old_right].spec_dispatch_id()));
        }
    }

    /// Every earlier worker has a strictly smaller complete identity.
    pub open spec fn spec_workers_ordered(&self) -> bool {
        Self::worker_records_ordered(self.spec_workers())
    }

    /// Every earlier work record has a strictly smaller complete identity.
    pub open spec fn spec_work_ordered(&self) -> bool {
        Self::work_records_ordered(self.spec_work())
    }

    /// Every earlier live reservation has a strictly smaller dispatch identity.
    pub open spec fn spec_reservations_ordered(&self) -> bool {
        Self::reservation_records_ordered(self.spec_reservations())
    }

    /// Every earlier historical dispatch identity is strictly smaller.
    pub open spec fn spec_used_dispatches_ordered(&self) -> bool {
        Self::dispatch_ids_ordered(self.spec_used_dispatches())
    }

    /// All scheduler collections consumed by canonical binary search are strictly ordered.
    pub open spec fn spec_collections_ordered(&self) -> bool {
        self.spec_workers_ordered() && self.spec_work_ordered()
            && self.spec_reservations_ordered() && self.spec_used_dispatches_ordered()
    }
}

} // verus!
