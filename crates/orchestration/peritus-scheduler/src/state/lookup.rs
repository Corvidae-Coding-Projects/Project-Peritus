//! Verified sound lookups using the production binary-search algorithm.

use vstd::prelude::*;

use super::SchedulerState;
use crate::{DispatchId, SchedulerReservation, WorkId, WorkRecord, WorkerId, WorkerRecord};

verus! {

impl SchedulerState {
    /// Locates a worker with the production binary-search algorithm.
    pub(crate) fn worker_index(&self, id: WorkerId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => {
                &&& index < self.spec_workers().len()
                &&& self.spec_workers()[index as int].spec_descriptor().spec_id() == id
            },
            None => true,
        },
    {
        let mut size: usize = self.workers.len();
        if size == 0 {
            return None;
        }
        let mut base: usize = 0;
        while size > 1
            invariant
                0 < size <= self.workers@.len(),
                base < self.workers@.len(),
                base + size <= self.workers@.len(),
            decreases size,
        {
            let half = size / 2;
            proof {
                assert(half < size);
                assert((base as int) + (size as int) <= self.workers.len() as int);
                assert(self.workers.len() as int <= usize::MAX as int);
                assert((base as int) + (half as int) <= usize::MAX as int);
            }
            let mid = base + half;
            let observed = self.workers[mid].descriptor().id();
            if !id.precedes(&observed) {
                base = mid;
            }
            size -= half;
        }
        if self.workers[base].descriptor().id().same(&id) {
            proof {
                assert(self.spec_workers()[base as int].spec_descriptor().spec_id() == id);
            }
            Some(base)
        } else {
            None
        }
    }

    /// Looks up a worker and proves every returned record has the requested identity.
    #[must_use]
    pub fn worker(&self, id: WorkerId) -> (result: Option<&WorkerRecord>)
        ensures match result {
            Some(record) => {
                &&& record.spec_descriptor().spec_id() == id
                &&& exists |index: int| #![trigger self.spec_workers()[index]]
                    0 <= index < self.spec_workers().len()
                        && self.spec_workers()[index].spec_descriptor().spec_id() == id
                        && self.spec_workers()[index].spec_phase() == record.spec_phase()
            },
            None => true,
        },
    {
        let found_index = self.worker_index(id)?;
        proof {
            assert(self.spec_workers()[found_index as int].spec_descriptor().spec_id() == id);
            assert(exists |index: int| #![trigger self.spec_workers()[index]]
                0 <= index < self.spec_workers().len()
                    && self.spec_workers()[index].spec_descriptor().spec_id() == id
                    && self.spec_workers()[index].spec_phase()
                        == self.spec_workers()[found_index as int].spec_phase()) by {
                assert(0 <= found_index as int
                    && (found_index as int) < self.spec_workers().len());
            }
        }
        Some(&self.workers[found_index])
    }

    /// Looks up work and proves every returned record has the requested identity.
    #[must_use]
    pub fn work_item(&self, id: WorkId) -> (result: Option<&WorkRecord>)
        ensures match result {
            Some(record) => {
                &&& record.spec_definition().spec_id() == id
                &&& exists |index: int| #![trigger self.spec_work()[index]]
                    0 <= index < self.spec_work().len()
                        && self.spec_work()[index].spec_definition().spec_id() == id
                        && self.spec_work()[index].spec_phase() == record.spec_phase()
            },
            None => true,
        },
    {
        let mut size: usize = self.work.len();
        if size == 0 {
            return None;
        }
        let mut base: usize = 0;
        while size > 1
            invariant
                0 < size <= self.work@.len(),
                base < self.work@.len(),
                base + size <= self.work@.len(),
            decreases size,
        {
            let half = size / 2;
            proof {
                assert(half < size);
                assert((base as int) + (size as int) <= self.work.len() as int);
                assert(self.work.len() as int <= usize::MAX as int);
                assert((base as int) + (half as int) <= usize::MAX as int);
            }
            let mid = base + half;
            let observed = self.work[mid].spec().id();
            if !id.precedes(&observed) {
                base = mid;
            }
            size -= half;
        }
        if self.work[base].spec().id().same(&id) {
            proof {
                assert(self.spec_work()[base as int].spec_definition().spec_id() == id);
                assert(exists |index: int| #![trigger self.spec_work()[index]]
                    0 <= index < self.spec_work().len()
                        && self.spec_work()[index].spec_definition().spec_id() == id
                        && self.spec_work()[index].spec_phase()
                            == self.spec_work()[base as int].spec_phase()) by {
                    assert(0 <= base as int && (base as int) < self.spec_work().len());
                }
            }
            Some(&self.work[base])
        } else {
            None
        }
    }

    /// Looks up a live reservation and proves every returned record has the requested identity.
    #[must_use]
    pub fn reservation(&self, id: DispatchId) -> (result: Option<&SchedulerReservation>)
        ensures match result {
            Some(reservation) => {
                &&& reservation.spec_dispatch_id() == id
                &&& exists |index: int| #![trigger self.spec_reservations()[index]]
                    0 <= index < self.spec_reservations().len()
                        && self.spec_reservations()[index].spec_dispatch_id() == id
                        && self.spec_reservations()[index].spec_work_id()
                            == reservation.spec_work_id()
                        && self.spec_reservations()[index].spec_started()
                            == reservation.spec_started()
            },
            None => true,
        },
    {
        let mut size: usize = self.reservations.len();
        if size == 0 {
            return None;
        }
        let mut base: usize = 0;
        while size > 1
            invariant
                0 < size <= self.reservations@.len(),
                base < self.reservations@.len(),
                base + size <= self.reservations@.len(),
            decreases size,
        {
            let half = size / 2;
            proof {
                assert(half < size);
                assert((base as int) + (size as int) <= self.reservations.len() as int);
                assert(self.reservations.len() as int <= usize::MAX as int);
                assert((base as int) + (half as int) <= usize::MAX as int);
            }
            let mid = base + half;
            let observed = self.reservations[mid].dispatch_id();
            if !id.precedes(&observed) {
                base = mid;
            }
            size -= half;
        }
        if self.reservations[base].dispatch_id().same(&id) {
            proof {
                assert(self.spec_reservations()[base as int].spec_dispatch_id() == id);
                assert(exists |index: int| #![trigger self.spec_reservations()[index]]
                    0 <= index < self.spec_reservations().len()
                        && self.spec_reservations()[index].spec_dispatch_id() == id
                        && self.spec_reservations()[index].spec_work_id()
                            == self.spec_reservations()[base as int].spec_work_id()
                        && self.spec_reservations()[index].spec_started()
                            == self.spec_reservations()[base as int].spec_started()) by {
                    assert(0 <= base as int
                        && (base as int) < self.spec_reservations().len());
                }
            }
            Some(&self.reservations[base])
        } else {
            None
        }
    }
}

} // verus!
