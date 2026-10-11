//! Immutable run/revision/capacity binding.

use peritus_types::{RevisionTuple, RunId, Sha256Digest};

use crate::{ResourceVector, SchedulerError, SchedulerId, SchedulerLimits, SchedulerSemantics};

#[doc = " Immutable identity and capacity context for one scheduler aggregate."]
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
    #[doc = " Returns the bound run."]
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[doc = " Returns the stable scheduler identity."]
    #[must_use]
    pub const fn scheduler_id(&self) -> SchedulerId {
        self.scheduler_id
    }
    #[doc = " Returns the exact immutable revision fence."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the immutable queue and recovery semantics."]
    #[must_use]
    pub const fn semantics(&self) -> SchedulerSemantics {
        self.semantics
    }
    #[doc = " Returns immutable independent bounds."]
    #[must_use]
    pub const fn limits(&self) -> SchedulerLimits {
        self.limits
    }
    #[doc = " Borrows total scheduler capacity."]
    #[must_use]
    pub const fn capacity(&self) -> &ResourceVector {
        &self.capacity
    }
}
impl Clone for SchedulerBinding {
    fn clone(&self) -> Self {
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
