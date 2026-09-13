//! Worker descriptors, lifecycle, and durable reservations.

use peritus_types::{ActorId, RevisionTuple, Sha256Digest};

use crate::{
    AttemptNumber, DispatchId, ExecutionClass, ResourceVector, SchedulerError, SchedulerErrorKind,
    SchedulerLimits, WorkId, WorkerId,
};

/// Immutable worker capabilities and ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerDescriptor {
    id: WorkerId,
    owner: ActorId,
    classes: Vec<ExecutionClass>,
    capacity: ResourceVector,
    concurrency: u16,
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
        if classes.is_empty()
            || classes.windows(2).any(|pair| pair[0] >= pair[1])
            || concurrency == 0
            || concurrency > limits.active_reservations()
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::NonCanonical,
                "worker classes or concurrency are empty, duplicated, unsorted, or out of bounds",
            ));
        }
        capacity.validate(limits.resource_dimensions())?;
        Ok(Self { id, owner, classes, capacity, concurrency })
    }

    /// Returns the worker identity.
    #[must_use]
    pub const fn id(&self) -> WorkerId {
        self.id
    }
    /// Returns the actor that owns dispatched work.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Borrows supported execution classes in canonical order.
    #[must_use]
    pub fn classes(&self) -> &[ExecutionClass] {
        &self.classes
    }
    /// Borrows worker-local capacity.
    #[must_use]
    pub const fn capacity(&self) -> &ResourceVector {
        &self.capacity
    }
    /// Returns maximum concurrent dispatch ownership.
    #[must_use]
    pub const fn concurrency(&self) -> u16 {
        self.concurrency
    }
    /// Returns whether this worker supports the class.
    #[must_use]
    pub fn supports(&self, class: ExecutionClass) -> bool {
        self.classes.binary_search(&class).is_ok()
    }
}

/// Closed worker availability lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkerPhase {
    /// Worker can receive at least one more dispatch.
    Available,
    /// Worker has exhausted concurrency or resource capacity.
    Busy,
    /// Worker retains active ownership but accepts no new work.
    Draining,
    /// Worker ownership was lost and its reservations were classified.
    Lost,
    /// Worker was explicitly removed after reaching quiescence.
    Removed,
}

/// Retained worker descriptor and current phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerRecord {
    descriptor: WorkerDescriptor,
    phase: WorkerPhase,
}

impl WorkerRecord {
    pub(crate) const fn new(descriptor: WorkerDescriptor) -> Self {
        Self { descriptor, phase: WorkerPhase::Available }
    }
    /// Borrows immutable descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &WorkerDescriptor {
        &self.descriptor
    }
    /// Returns current lifecycle.
    #[must_use]
    pub const fn phase(&self) -> WorkerPhase {
        self.phase
    }
    pub(crate) const fn set_phase(&mut self, phase: WorkerPhase) {
        self.phase = phase;
    }
    pub(crate) const fn from_wire(descriptor: WorkerDescriptor, phase: WorkerPhase) -> Self {
        Self { descriptor, phase }
    }
}

/// Exact durable reservation and inert dispatch directive.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Creates an exact unacknowledged reservation observation.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "reservation bindings remain explicit")]
    pub const fn new(
        work_id: WorkId,
        dispatch_id: DispatchId,
        worker_id: WorkerId,
        owner: ActorId,
        attempt: AttemptNumber,
        revision: RevisionTuple,
        resources: ResourceVector,
        dispatch_token: Sha256Digest,
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
    /// Returns bound work.
    #[must_use]
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    /// Returns durable dispatch identity.
    #[must_use]
    pub const fn dispatch_id(&self) -> DispatchId {
        self.dispatch_id
    }
    /// Returns assigned worker.
    #[must_use]
    pub const fn worker_id(&self) -> WorkerId {
        self.worker_id
    }
    /// Returns assigned owning actor.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Returns one-based work attempt.
    #[must_use]
    pub const fn attempt(&self) -> AttemptNumber {
        self.attempt
    }
    /// Returns exact revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows exact reserved resources.
    #[must_use]
    pub const fn resources(&self) -> &ResourceVector {
        &self.resources
    }
    /// Returns idempotent effect token.
    #[must_use]
    pub const fn dispatch_token(&self) -> Sha256Digest {
        self.dispatch_token
    }
    /// Returns whether the owner acknowledged start.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }
    pub(crate) const fn mark_started(&mut self) {
        self.started = true;
    }
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
