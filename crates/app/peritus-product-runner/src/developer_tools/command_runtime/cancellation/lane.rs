//! Retained scheduler lanes and exact logical-job admission.

use std::{
    collections::{BTreeSet, VecDeque},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use super::CancellationTask;

#[derive(Default)]
pub(super) struct TaskOwners {
    retained: Mutex<BTreeSet<String>>,
}

impl TaskOwners {
    pub(super) fn admit(&self, task: &CancellationTask) -> bool {
        self.retained
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(task.handle.clone())
    }

    pub(super) fn retire(&self, task: &CancellationTask) {
        self.retained
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&task.handle);
    }
}

#[derive(Default)]
pub(super) struct TaskLane {
    pending: Mutex<VecDeque<CancellationTask>>,
    wake: Condvar,
    closed: AtomicBool,
}

impl TaskLane {
    // Transfers preserve the TaskOwners entry. Only initial admission and final settlement
    // change it, so a retry or worker-spawn failure cannot mint another logical job.
    pub(super) fn enqueue(&self, task: CancellationTask) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(task);
        self.wake.notify_one();
    }

    pub(super) fn take(&self) -> Option<CancellationTask> {
        let mut pending =
            self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(task) = pending.pop_front() {
                return Some(task);
            }
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            pending = self
                .wake
                .wait(pending)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.wake.notify_all();
    }
}
