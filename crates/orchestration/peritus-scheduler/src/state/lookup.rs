//! Exact lookups in canonical state using the production binary-search algorithm.

use super::SchedulerState;
use crate::{DispatchId, SchedulerReservation, WorkId, WorkRecord, WorkerId, WorkerRecord};

impl SchedulerState {
    #[doc = " Locates a worker with the production binary-search algorithm."]
    pub(crate) fn worker_index(&self, id: WorkerId) -> Option<usize> {
        let mut size: usize = self.workers.len();
        if size == 0 {
            return None;
        }
        let mut base: usize = 0;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            let observed = self.workers[mid].descriptor().id();
            if !id.precedes(&observed) {
                base = mid;
            }
            size -= half;
        }
        if self.workers[base].descriptor().id().same(&id) { Some(base) } else { None }
    }
    #[doc = " Returns the exact retained worker; canonical ordering makes absence complete."]
    #[must_use]
    pub fn worker(&self, id: WorkerId) -> Option<&WorkerRecord> {
        let found_index = self.worker_index(id)?;
        Some(&self.workers[found_index])
    }
    #[doc = " Returns the exact retained work; canonical ordering makes absence complete."]
    #[must_use]
    pub fn work_item(&self, id: WorkId) -> Option<&WorkRecord> {
        let mut size: usize = self.work.len();
        if size == 0 {
            return None;
        }
        let mut base: usize = 0;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            let observed = self.work[mid].spec().id();
            if !id.precedes(&observed) {
                base = mid;
            }
            size -= half;
        }
        if self.work[base].spec().id().same(&id) { Some(&self.work[base]) } else { None }
    }
    #[doc = " Returns the exact live reservation; canonical ordering makes absence complete."]
    #[must_use]
    pub fn reservation(&self, id: DispatchId) -> Option<&SchedulerReservation> {
        let mut size: usize = self.reservations.len();
        if size == 0 {
            return None;
        }
        let mut base: usize = 0;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            let observed = self.reservations[mid].dispatch_id();
            if !id.precedes(&observed) {
                base = mid;
            }
            size -= half;
        }
        if self.reservations[base].dispatch_id().same(&id) {
            Some(&self.reservations[base])
        } else {
            None
        }
    }
}
