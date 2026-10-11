//! Worker descriptors, lifecycle, and durable reservations.

use peritus_types::{ActorId, RevisionTuple, Sha256Digest};

use crate::{
    AttemptNumber, DispatchId, ExecutionClass, ResourceVector, SchedulerError, SchedulerErrorKind,
    SchedulerLimits, WorkId, WorkerId,
};

mod admission;
mod clone_impl;

#[doc = " Immutable worker capabilities and ownership."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkerDescriptor {
    id: WorkerId,
    owner: ActorId,
    classes: Vec<ExecutionClass>,
    capacity: ResourceVector,
    concurrency: u16,
}
impl WorkerDescriptor {
    #[doc = " Returns the worker identity."]
    #[must_use]
    pub const fn id(&self) -> WorkerId {
        self.id
    }
    #[doc = " Returns the actor that owns dispatched work."]
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    #[doc = " Borrows worker-local capacity."]
    #[must_use]
    pub const fn capacity(&self) -> &ResourceVector {
        &self.capacity
    }
    #[doc = " Returns maximum concurrent dispatch ownership."]
    #[must_use]
    pub const fn concurrency(&self) -> u16 {
        self.concurrency
    }
    #[doc = " Borrows supported execution classes in canonical order."]
    #[must_use]
    pub fn classes(&self) -> &[ExecutionClass] {
        &self.classes
    }
    #[doc = " Returns whether this worker supports the class."]
    #[must_use]
    pub fn supports(&self, class: ExecutionClass) -> bool {
        let mut index = 0;
        while index < self.classes.len() {
            if self.classes[index].same(class) {
                return true;
            }
            index += 1;
        }
        false
    }
}

impl WorkerDescriptor {
    /// Creates a checked worker descriptor.
    ///
    /// # Errors
    /// Rejects empty/noncanonical classes, invalid capacity, or zero/excess concurrency.
    pub fn new(
        id: WorkerId,
        owner: ActorId,
        classes: Vec<ExecutionClass>,
        capacity: ResourceVector,
        concurrency: u16,
        limits: SchedulerLimits,
    ) -> Result<Self, SchedulerError> {
        match admission::admit_descriptor(
            id,
            owner,
            classes,
            capacity,
            concurrency,
            limits.active_reservations(),
            limits.resource_dimensions(),
        ) {
            admission::DescriptorAdmission::Accepted(descriptor) => Ok(descriptor),
            admission::DescriptorAdmission::ClassesOrConcurrencyRejected => {
                Err(crate::error::reject(
                    SchedulerErrorKind::NonCanonical,
                    "worker classes or concurrency are empty, duplicated, unsorted, or out of bounds",
                ))
            }
            admission::DescriptorAdmission::CapacityLimitExceeded => Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "resource vector is empty or exceeds its dimension bound",
            )),
        }
    }
}

#[doc = " Closed worker availability lifecycle."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkerPhase {
    #[doc = " Worker can receive at least one more dispatch."]
    Available,
    #[doc = " Worker has exhausted concurrency or resource capacity."]
    Busy,
    #[doc = " Worker retains active ownership but accepts no new work."]
    Draining,
    #[doc = " Worker ownership was lost and its reservations were classified."]
    Lost,
    #[doc = " Worker was explicitly removed after reaching quiescence."]
    Removed,
}
impl WorkerPhase {
    #[doc = " Compares exact lifecycle variants for verified production admission."]
    pub(crate) const fn same(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Available, Self::Available)
                | (Self::Busy, Self::Busy)
                | (Self::Draining, Self::Draining)
                | (Self::Lost, Self::Lost)
                | (Self::Removed, Self::Removed)
        )
    }
}

#[doc = " Retained worker descriptor and current phase."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkerRecord {
    descriptor: WorkerDescriptor,
    phase: WorkerPhase,
}
impl WorkerRecord {
    #[doc = " Borrows immutable descriptor."]
    #[must_use]
    pub const fn descriptor(&self) -> &WorkerDescriptor {
        &self.descriptor
    }
    #[doc = " Returns current lifecycle."]
    #[must_use]
    pub const fn phase(&self) -> WorkerPhase {
        self.phase
    }
    pub(crate) const fn set_phase(&mut self, phase: WorkerPhase) {
        self.phase = phase;
    }
}

impl WorkerRecord {
    pub(crate) const fn new(descriptor: WorkerDescriptor) -> Self {
        Self { descriptor, phase: WorkerPhase::Available }
    }
    pub(crate) const fn from_wire(descriptor: WorkerDescriptor, phase: WorkerPhase) -> Self {
        Self { descriptor, phase }
    }
}

#[doc = " Exact durable reservation and inert dispatch directive."]
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerReservation {
    work_id: WorkId,
    dispatch_id: DispatchId,
    worker_id: WorkerId,
    owner: ActorId,
    attempt: AttemptNumber,
    revision: RevisionTuple,
    resources: ResourceVector,
    dispatch_token: Sha256Digest,
    started: bool,
}

impl SchedulerReservation {
    #[doc = " Returns idempotent effect token."]
    #[must_use]
    pub const fn dispatch_token(&self) -> Sha256Digest {
        self.dispatch_token
    }
}

impl SchedulerReservation {
    pub(crate) fn validate_against(
        &self,
        work: &crate::WorkRecord,
        worker: &WorkerRecord,
    ) -> Result<(), SchedulerError> {
        if self.work_id != work.spec().id()
            || self.worker_id != worker.descriptor().id()
            || self.owner != worker.descriptor().owner()
            || self.owner != work.spec().owner()
            || self.revision != work.spec().revision()
            || &self.resources != work.spec().request()
            || !self.resources.fits_within(worker.descriptor().capacity())
            || self.attempt.get() != work.attempts_started()
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::BindingMismatch,
                "reservation differs from work, worker, owner, attempt, revision, or resources",
            ));
        }
        Ok(())
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "exact closed-wire reservation fields are reconstructed without defaults"
    )]
    pub(crate) const fn from_wire(
        work_id: WorkId,
        dispatch_id: DispatchId,
        worker_id: WorkerId,
        owner: ActorId,
        attempt: AttemptNumber,
        revision: RevisionTuple,
        resources: ResourceVector,
        dispatch_token: Sha256Digest,
        started: bool,
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
            started,
        }
    }
}
