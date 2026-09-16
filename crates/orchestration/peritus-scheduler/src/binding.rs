//! Immutable run/revision/capacity binding.

use peritus_types::{RevisionTuple, RunId, Sha256Digest};

use crate::{ResourceVector, SchedulerError, SchedulerId, SchedulerLimits, SchedulerSemantics};
use vstd::prelude::*;

verus! {

/// Immutable identity and capacity context for one scheduler aggregate.
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerBinding {
    semantics: SchedulerSemantics,
    run_id: RunId,
    scheduler_id: SchedulerId,
    revision: RevisionTuple,
    limits: SchedulerLimits,
    capacity: ResourceVector,
}

impl SchedulerBinding {
    /// Returns the immutable scheduler semantics.
    pub closed spec fn spec_semantics(&self) -> SchedulerSemantics { self.semantics }

    /// Returns the exact immutable run identity.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }

    /// Returns the exact immutable scheduler identity.
    pub closed spec fn spec_scheduler_id(&self) -> SchedulerId { self.scheduler_id }

    /// Returns the exact immutable revision fence.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Returns the bound run.
    #[must_use]
    pub const fn run_id(&self) -> (result: RunId)
        ensures result == self.spec_run_id(),
    { self.run_id }

    /// Returns the stable scheduler identity.
    #[must_use]
    pub const fn scheduler_id(&self) -> (result: SchedulerId)
        ensures result == self.spec_scheduler_id(),
    { self.scheduler_id }

    /// Returns the exact immutable revision fence.
    #[must_use]
    pub const fn revision(&self) -> (result: RevisionTuple)
        ensures result == self.spec_revision(),
    { self.revision }

    /// Relates an exact semantic clone of every immutable binding field.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.semantics == right.semantics
            && left.run_id == right.run_id
            && left.scheduler_id == right.scheduler_id
            && left.revision == right.revision
            && left.limits == right.limits
            && left.capacity.spec_entries() == right.capacity.spec_entries()
    }

    /// Projects the reservation model fields preserved by a complete clone.
    pub proof fn clone_preserves_reservation_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            left.spec_semantics() == right.spec_semantics(),
            left.spec_limits() == right.spec_limits(),
            left.spec_capacity().spec_entries() == right.spec_capacity().spec_entries(),
    {
    }

    /// Returns the immutable queue and recovery semantics.
    #[must_use]
    pub const fn semantics(&self) -> (result: SchedulerSemantics)
        ensures result == self.spec_semantics(),
    {
        self.semantics
    }

    /// Returns the mathematical scheduler limits.
    pub closed spec fn spec_limits(&self) -> SchedulerLimits {
        self.limits
    }

    /// Returns the mathematical global resource capacity.
    pub closed spec fn spec_capacity(&self) -> &ResourceVector {
        &self.capacity
    }

    /// Returns immutable independent bounds.
    #[must_use]
    pub const fn limits(&self) -> (result: SchedulerLimits)
        ensures result == self.spec_limits(),
    {
        self.limits
    }

    /// Borrows total scheduler capacity.
    #[must_use]
    pub const fn capacity(&self) -> (result: &ResourceVector)
        ensures result == self.spec_capacity(),
    {
        &self.capacity
    }
}

impl Clone for SchedulerBinding {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            semantics: self.semantics,
            run_id: self.run_id,
            scheduler_id: self.scheduler_id,
            revision: self.revision,
            limits: self.limits,
            capacity: self.capacity.clone(),
        }
    }
}

} // verus!

impl SchedulerBinding {
    /// Creates a complete checked scheduler binding.
    ///
    /// # Errors
    /// Rejects invalid limits or a noncanonical/oversized capacity vector.
    pub fn new(
        run_id: RunId,
        scheduler_id: SchedulerId,
        revision: RevisionTuple,
        limits: SchedulerLimits,
        capacity: ResourceVector,
    ) -> Result<Self, SchedulerError> {
        Self::from_wire(
            SchedulerSemantics::StrictRecoveryQueueV2,
            run_id,
            scheduler_id,
            revision,
            limits,
            capacity,
        )
    }

    pub(crate) fn from_wire(
        semantics: SchedulerSemantics,
        run_id: RunId,
        scheduler_id: SchedulerId,
        revision: RevisionTuple,
        limits: SchedulerLimits,
        capacity: ResourceVector,
    ) -> Result<Self, SchedulerError> {
        limits.validate()?;
        capacity.validate(limits.resource_dimensions())?;
        Ok(Self { semantics, run_id, scheduler_id, revision, limits, capacity })
    }

    /// Returns the domain-separated canonical digest of every immutable binding field.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        crate::canonical::binding_digest(self)
    }

    pub(crate) fn validate(&self) -> Result<(), SchedulerError> {
        Self::from_wire(
            self.semantics,
            self.run_id,
            self.scheduler_id,
            self.revision,
            self.limits,
            self.capacity.clone(),
        )
        .map(|_| ())
    }
}
