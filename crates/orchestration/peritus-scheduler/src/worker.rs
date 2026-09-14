//! Worker descriptors, lifecycle, and durable reservations.

use peritus_types::{ActorId, RevisionTuple, Sha256Digest};

use crate::{
    AttemptNumber, DispatchId, ExecutionClass, ResourceVector, SchedulerError, SchedulerErrorKind,
    SchedulerLimits, WorkId, WorkerId,
};
use vstd::prelude::*;

mod admission;
mod clone_impl;

verus! {

/// Immutable worker capabilities and ownership.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkerDescriptor {
    id: WorkerId,
    owner: ActorId,
    classes: Vec<ExecutionClass>,
    capacity: ResourceVector,
    concurrency: u16,
}

impl WorkerDescriptor {
    /// Returns the mathematical worker identity.
    pub closed spec fn spec_id(&self) -> WorkerId { self.id }
    /// Returns the mathematical worker owner.
    pub closed spec fn spec_owner(&self) -> ActorId { self.owner }
    /// Returns the mathematical worker resource capacity.
    pub closed spec fn spec_capacity(&self) -> &ResourceVector { &self.capacity }
    /// Returns the mathematical worker concurrency ceiling.
    pub closed spec fn spec_concurrency(&self) -> u16 { self.concurrency }
    /// Returns the mathematical supported execution classes.
    pub closed spec fn spec_classes(&self) -> Seq<ExecutionClass> { self.classes@ }

    /// Returns the worker identity.
    #[must_use]
    pub const fn id(&self) -> (result: WorkerId)
        ensures result == self.spec_id(),
    {
        self.id
    }

    /// Returns the actor that owns dispatched work.
    #[must_use]
    pub const fn owner(&self) -> (result: ActorId)
        ensures result == self.spec_owner(),
    {
        self.owner
    }

    /// Borrows worker-local capacity.
    #[must_use]
    pub const fn capacity(&self) -> (result: &ResourceVector)
        ensures result == self.spec_capacity(),
    {
        &self.capacity
    }

    /// Returns maximum concurrent dispatch ownership.
    #[must_use]
    pub const fn concurrency(&self) -> (result: u16)
        ensures result == self.spec_concurrency(),
    {
        self.concurrency
    }

    /// Borrows supported execution classes in canonical order.
    #[must_use]
    pub fn classes(&self) -> (result: &[ExecutionClass])
        ensures result@ == self.spec_classes(),
    {
        &self.classes
    }

    /// Returns whether this worker supports the class.
    #[must_use]
    pub fn supports(&self, class: ExecutionClass) -> (result: bool)
        ensures result == self.spec_classes().contains(class),
    {
        let mut index = 0;
        while index < self.classes.len()
            invariant
                index <= self.classes@.len(),
                forall |prior: int| 0 <= prior < index ==> self.classes@[prior] != class,
            decreases self.classes@.len() - index,
        {
            if self.classes[index].same(class) {
                proof {
                    assert(self.classes@[index as int] == class);
                }
                return true;
            }
            proof {
                assert(self.classes@[index as int] != class);
            }
            index += 1;
        }
        false
    }
}

} // verus!

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

verus! {

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

impl WorkerPhase {
    /// Compares exact lifecycle variants for verified production admission.
    pub(crate) const fn same(self, other: Self) -> (result: bool)
        ensures result == (self == other),
    {
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

} // verus!

verus! {

/// Retained worker descriptor and current phase.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkerRecord {
    descriptor: WorkerDescriptor,
    phase: WorkerPhase,
}

impl WorkerRecord {
    /// Returns the mathematical worker descriptor.
    pub closed spec fn spec_descriptor(&self) -> &WorkerDescriptor { &self.descriptor }
    /// Returns the mathematical worker lifecycle.
    pub closed spec fn spec_phase(&self) -> WorkerPhase { self.phase }

    /// Borrows immutable descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> (result: &WorkerDescriptor)
        ensures result == self.spec_descriptor(),
    {
        &self.descriptor
    }

    /// Returns current lifecycle.
    #[must_use]
    pub const fn phase(&self) -> (result: WorkerPhase)
        ensures result == self.spec_phase(),
    {
        self.phase
    }

    /// Relates worker fields used by the reservation invariant.
    pub closed spec fn reservation_owner_equivalent(left: &Self, right: &Self) -> bool {
        left.descriptor.id == right.descriptor.id
            && left.descriptor.owner == right.descriptor.owner
            && left.descriptor.capacity.spec_entries()
                == right.descriptor.capacity.spec_entries()
            && left.descriptor.concurrency == right.descriptor.concurrency
    }

    pub(crate) proof fn reservation_owner_fields(left: &Self, right: &Self)
        requires Self::reservation_owner_equivalent(left, right),
        ensures
            left.spec_descriptor().spec_id() == right.spec_descriptor().spec_id(),
            left.spec_descriptor().spec_owner() == right.spec_descriptor().spec_owner(),
            left.spec_descriptor().spec_capacity().spec_entries()
                == right.spec_descriptor().spec_capacity().spec_entries(),
            left.spec_descriptor().spec_concurrency()
                == right.spec_descriptor().spec_concurrency(),
    {
    }

    pub(crate) proof fn reservation_owner_reflexive(value: &Self)
        ensures Self::reservation_owner_equivalent(value, value),
    {
    }

    pub(crate) const fn set_phase(&mut self, phase: WorkerPhase)
        ensures
            Self::reservation_owner_equivalent(old(self), final(self)),
            final(self).spec_descriptor() == old(self).spec_descriptor(),
            final(self).spec_phase() == phase,
    {
        self.phase = phase;
    }
}

} // verus!

impl WorkerRecord {
    pub(crate) const fn new(descriptor: WorkerDescriptor) -> Self {
        Self { descriptor, phase: WorkerPhase::Available }
    }
    pub(crate) const fn from_wire(descriptor: WorkerDescriptor, phase: WorkerPhase) -> Self {
        Self { descriptor, phase }
    }
}

verus! {

/// Exact durable reservation and inert dispatch directive.
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

} // verus!

verus! {

impl SchedulerReservation {
    /// Returns the mathematical idempotent effect token.
    pub closed spec fn spec_dispatch_token(&self) -> Sha256Digest { self.dispatch_token }

    /// Returns idempotent effect token.
    #[must_use]
    pub const fn dispatch_token(&self) -> (result: Sha256Digest)
        ensures result == self.spec_dispatch_token(),
    {
        self.dispatch_token
    }
}

} // verus!

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
