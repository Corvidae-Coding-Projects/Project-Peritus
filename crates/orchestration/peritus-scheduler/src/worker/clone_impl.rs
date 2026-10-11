//! Verified semantic clones for workers and durable reservations.

use super::{SchedulerReservation, WorkerDescriptor, WorkerRecord};

impl WorkerDescriptor {}
impl Clone for WorkerDescriptor {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            owner: self.owner,
            classes: self.classes.clone(),
            capacity: self.capacity.clone(),
            concurrency: self.concurrency,
        }
    }
}
impl WorkerRecord {}
impl Clone for WorkerRecord {
    fn clone(&self) -> Self {
        Self { descriptor: self.descriptor.clone(), phase: self.phase }
    }
}
impl SchedulerReservation {
    #[doc = " Returns bound work."]
    #[must_use]
    pub const fn work_id(&self) -> crate::WorkId {
        self.work_id
    }
    #[doc = " Returns durable dispatch identity."]
    #[must_use]
    pub const fn dispatch_id(&self) -> crate::DispatchId {
        self.dispatch_id
    }
    #[doc = " Creates an exact unacknowledged reservation observation."]
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "reservation bindings remain explicit")]
    pub const fn new(
        work_id: crate::WorkId,
        dispatch_id: crate::DispatchId,
        worker_id: crate::WorkerId,
        owner: peritus_types::ActorId,
        attempt: crate::AttemptNumber,
        revision: peritus_types::RevisionTuple,
        resources: crate::ResourceVector,
        dispatch_token: peritus_types::Sha256Digest,
    ) -> Self {
        Self {
            work_id,
            dispatch_id,
            worker_id,
            owner,
            attempt,
            revision,
            resources,
            dispatch_token,
            started: false,
        }
    }
    #[doc = " Returns assigned worker."]
    #[must_use]
    pub const fn worker_id(&self) -> crate::WorkerId {
        self.worker_id
    }
    #[doc = " Returns assigned owning actor."]
    #[must_use]
    pub const fn owner(&self) -> peritus_types::ActorId {
        self.owner
    }
    #[doc = " Returns one-based work attempt."]
    #[must_use]
    pub const fn attempt(&self) -> crate::AttemptNumber {
        self.attempt
    }
    #[doc = " Returns exact revision."]
    #[must_use]
    pub const fn revision(&self) -> peritus_types::RevisionTuple {
        self.revision
    }
    #[doc = " Borrows exact reserved resources."]
    #[must_use]
    pub const fn resources(&self) -> &crate::ResourceVector {
        &self.resources
    }
    #[doc = " Returns whether the owner acknowledged start."]
    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }
    pub(crate) const fn mark_started(&mut self) {
        self.started = true;
    }
}
impl Clone for SchedulerReservation {
    fn clone(&self) -> Self {
        Self {
            work_id: self.work_id,
            dispatch_id: self.dispatch_id,
            worker_id: self.worker_id,
            owner: self.owner,
            attempt: self.attempt,
            revision: self.revision,
            resources: self.resources.clone(),
            dispatch_token: self.dispatch_token,
            started: self.started,
        }
    }
}
