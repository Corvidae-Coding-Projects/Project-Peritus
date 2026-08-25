//! Immutable run/revision/capacity binding.

use peritus_types::{RevisionTuple, RunId, Sha256Digest};

use crate::{ResourceVector, SchedulerError, SchedulerId, SchedulerLimits};

/// Immutable identity and capacity context for one scheduler aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerBinding {
    run_id: RunId,
    scheduler_id: SchedulerId,
    revision: RevisionTuple,
    limits: SchedulerLimits,
    capacity: ResourceVector,
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
        limits.validate()?;
        capacity.validate(limits.resource_dimensions())?;
        Ok(Self { run_id, scheduler_id, revision, limits, capacity })
    }

    /// Returns the bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the stable scheduler identity.
    #[must_use]
    pub const fn scheduler_id(&self) -> SchedulerId {
        self.scheduler_id
    }
    /// Returns the exact immutable revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns immutable independent bounds.
    #[must_use]
    pub const fn limits(&self) -> SchedulerLimits {
        self.limits
    }
    /// Borrows total scheduler capacity.
    #[must_use]
    pub const fn capacity(&self) -> &ResourceVector {
        &self.capacity
    }

    /// Returns the domain-separated canonical digest of every immutable binding field.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest {
        crate::canonical::binding_digest(self)
    }

    pub(crate) fn validate(&self) -> Result<(), SchedulerError> {
        Self::new(self.run_id, self.scheduler_id, self.revision, self.limits, self.capacity.clone())
            .map(|_| ())
    }
}
