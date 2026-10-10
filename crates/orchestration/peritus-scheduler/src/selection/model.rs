//! Exact mathematical model for production scheduler selection.

use crate::{WorkId, WorkerId};

#[doc = " One deterministic feasible work/worker choice."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub(super) work_id: WorkId,
    pub(super) worker_id: WorkerId,
}
impl Selection {
    pub(super) const fn new(work_id: WorkId, worker_id: WorkerId) -> Self {
        Self { work_id, worker_id }
    }
    #[doc = " Returns selected work."]
    #[must_use]
    pub const fn work_id(self) -> WorkId {
        self.work_id
    }
    #[doc = " Returns selected worker."]
    #[must_use]
    pub const fn worker_id(self) -> WorkerId {
        self.worker_id
    }
}
pub(super) struct IndexedSelection {
    pub(super) work_index: usize,
    pub(super) worker_index: usize,
}
