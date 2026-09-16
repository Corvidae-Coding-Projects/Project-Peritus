//! Verified semantic clones for workers and durable reservations.

use super::{SchedulerReservation, WorkerDescriptor, WorkerRecord};
use vstd::prelude::*;

verus! {

impl WorkerDescriptor {
    /// Relates an exact semantic clone of a worker descriptor.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.id == right.id
            && left.owner == right.owner
            && left.classes@ == right.classes@
            && left.capacity.spec_entries() == right.capacity.spec_entries()
            && left.concurrency == right.concurrency
    }
}

impl Clone for WorkerDescriptor {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            id: self.id,
            owner: self.owner,
            classes: self.classes.clone(),
            capacity: self.capacity.clone(),
            concurrency: self.concurrency,
        }
    }
}

impl WorkerRecord {
    /// Relates an exact semantic clone of a retained worker record.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        WorkerDescriptor::clone_equivalent(&left.descriptor, &right.descriptor)
            && left.phase == right.phase
    }

    pub(crate) proof fn clone_reservation_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            Self::reservation_owner_equivalent(left, right),
            left.spec_descriptor().spec_id() == right.spec_descriptor().spec_id(),
            left.spec_descriptor().spec_owner() == right.spec_descriptor().spec_owner(),
            left.spec_descriptor().spec_capacity().spec_entries()
                == right.spec_descriptor().spec_capacity().spec_entries(),
            left.spec_descriptor().spec_concurrency()
                == right.spec_descriptor().spec_concurrency(),
            left.spec_phase() == right.spec_phase(),
    {
        reveal(WorkerRecord::clone_equivalent);
        reveal(WorkerDescriptor::clone_equivalent);
        reveal(WorkerRecord::reservation_owner_equivalent);
    }
}

impl Clone for WorkerRecord {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self { descriptor: self.descriptor.clone(), phase: self.phase }
    }
}

impl SchedulerReservation {
    /// Returns the mathematical work identity.
    pub closed spec fn spec_work_id(&self) -> crate::WorkId { self.work_id }
    /// Returns bound work.
    #[must_use]
    pub const fn work_id(&self) -> (result: crate::WorkId)
        ensures result == self.spec_work_id(),
    {
        self.work_id
    }
    /// Returns the mathematical dispatch identity.
    pub closed spec fn spec_dispatch_id(&self) -> crate::DispatchId { self.dispatch_id }
    /// Returns durable dispatch identity.
    #[must_use]
    pub const fn dispatch_id(&self) -> (result: crate::DispatchId)
        ensures result == self.spec_dispatch_id(),
    {
        self.dispatch_id
    }
    /// Returns the mathematical worker identity.
    pub closed spec fn spec_worker_id(&self) -> crate::WorkerId { self.worker_id }
    /// Returns the mathematical owner identity.
    pub closed spec fn spec_owner(&self) -> peritus_types::ActorId { self.owner }
    /// Returns the mathematical attempt number.
    pub closed spec fn spec_attempt(&self) -> crate::AttemptNumber { self.attempt }
    /// Returns the mathematical revision fence.
    pub closed spec fn spec_revision(&self) -> peritus_types::RevisionTuple { self.revision }
    /// Returns the mathematical reserved resources.
    pub closed spec fn spec_resources(&self) -> &crate::ResourceVector { &self.resources }
    /// Returns whether the worker has acknowledged this reservation.
    pub closed spec fn spec_started(&self) -> bool { self.started }

    /// Creates an exact unacknowledged reservation observation.
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
    ) -> (result: Self)
        ensures
            result.spec_work_id() == work_id,
            result.spec_dispatch_id() == dispatch_id,
            result.spec_worker_id() == worker_id,
            crate::identity::actor_ids_match(result.spec_owner(), owner),
            result.spec_attempt() == attempt,
            result.spec_revision() == revision,
            result.spec_resources().spec_entries() == resources.spec_entries(),
            result.spec_dispatch_token() == dispatch_token,
            !result.spec_started(),
    {
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

    /// Returns assigned worker.
    #[must_use]
    pub const fn worker_id(&self) -> (result: crate::WorkerId)
        ensures result == self.spec_worker_id(),
    {
        self.worker_id
    }

    /// Returns assigned owning actor.
    #[must_use]
    pub const fn owner(&self) -> (result: peritus_types::ActorId)
        ensures result == self.spec_owner(),
    {
        self.owner
    }

    /// Returns one-based work attempt.
    #[must_use]
    pub const fn attempt(&self) -> (result: crate::AttemptNumber)
        ensures result == self.spec_attempt(),
    {
        self.attempt
    }

    /// Returns exact revision.
    #[must_use]
    pub const fn revision(&self) -> (result: peritus_types::RevisionTuple)
        ensures result == self.spec_revision(),
    {
        self.revision
    }

    /// Borrows exact reserved resources.
    #[must_use]
    pub const fn resources(&self) -> (result: &crate::ResourceVector)
        ensures result == self.spec_resources(),
    {
        &self.resources
    }

    /// Returns whether the owner acknowledged start.
    #[must_use]
    pub const fn started(&self) -> (result: bool)
        ensures result == self.spec_started(),
    {
        self.started
    }

    /// Relates an exact semantic clone of a durable reservation.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.work_id == right.work_id
            && left.dispatch_id == right.dispatch_id
            && left.worker_id == right.worker_id
            && left.owner == right.owner
            && left.attempt == right.attempt
            && left.revision == right.revision
            && left.resources.spec_entries() == right.resources.spec_entries()
            && left.dispatch_token == right.dispatch_token
            && left.started == right.started
    }

    /// Relates every reservation field used by the capacity and ownership invariant.
    pub closed spec fn invariant_equivalent(left: &Self, right: &Self) -> bool {
        left.work_id == right.work_id
            && left.dispatch_id == right.dispatch_id
            && left.worker_id == right.worker_id
            && left.owner == right.owner
            && left.attempt == right.attempt
            && left.revision == right.revision
            && left.resources.spec_entries() == right.resources.spec_entries()
    }

    pub(crate) proof fn invariant_fields(left: &Self, right: &Self)
        requires Self::invariant_equivalent(left, right),
        ensures
            left.spec_work_id() == right.spec_work_id(),
            left.spec_dispatch_id() == right.spec_dispatch_id(),
            left.spec_worker_id() == right.spec_worker_id(),
            left.spec_owner() == right.spec_owner(),
            left.spec_attempt() == right.spec_attempt(),
            left.spec_revision() == right.spec_revision(),
            left.spec_resources().spec_entries() == right.spec_resources().spec_entries(),
    {
    }

    pub(crate) proof fn invariant_reflexive(value: &Self)
        ensures Self::invariant_equivalent(value, value),
    {
    }

    pub(crate) proof fn clone_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            Self::invariant_equivalent(left, right),
            left.spec_work_id() == right.spec_work_id(),
            left.spec_dispatch_id() == right.spec_dispatch_id(),
            left.spec_worker_id() == right.spec_worker_id(),
            left.spec_owner() == right.spec_owner(),
            left.spec_attempt() == right.spec_attempt(),
            left.spec_revision() == right.spec_revision(),
            left.spec_resources().spec_entries() == right.spec_resources().spec_entries(),
            left.spec_started() == right.spec_started(),
    {
    }

    pub(crate) const fn mark_started(&mut self)
        ensures
            Self::invariant_equivalent(old(self), final(self)),
            final(self).spec_started(),
    {
        self.started = true;
    }
}

impl Clone for SchedulerReservation {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
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

} // verus!
