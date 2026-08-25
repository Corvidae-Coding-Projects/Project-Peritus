//! Immutable run, scheduler, revision, root, and limit binding.

use peritus_scheduler::SchedulerId;
use peritus_types::{RevisionTuple, RunId, Sha256Digest};

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::{CollaborationId, CollaborationLimits, CollaborationTaskId, Delegation};

/// Complete immutable binding used by every collaboration transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationBinding {
    id: CollaborationId,
    run_id: RunId,
    revision: RevisionTuple,
    scheduler_id: SchedulerId,
    root_task_id: CollaborationTaskId,
    limits: CollaborationLimits,
    root_assignment: Delegation,
    digest: Sha256Digest,
}

impl CollaborationBinding {
    /// Creates a complete checked collaboration binding.
    ///
    /// # Errors
    /// Rejects a root assignment whose causal shape does not describe the named root.
    pub fn new(
        id: CollaborationId,
        run_id: RunId,
        revision: RevisionTuple,
        scheduler_id: SchedulerId,
        limits: CollaborationLimits,
        root_assignment: Delegation,
    ) -> Result<Self, CollaborationError> {
        if root_assignment.task_id() != root_assignment.root_task_id()
            || root_assignment.parent_task_id().is_some()
            || root_assignment.depth() != 0
            || !root_assignment.required()
        {
            return Err(reject(
                CollaborationErrorKind::BindingMismatch,
                "collaboration root assignment has invalid root causality",
            ));
        }
        let mut binding = Self::from_wire(
            id,
            run_id,
            revision,
            scheduler_id,
            root_assignment.task_id(),
            limits,
            root_assignment,
            Sha256Digest::new([0; 32]),
        );
        binding.digest = crate::canonical::binding_digest(&binding);
        Ok(binding)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        id: CollaborationId,
        run_id: RunId,
        revision: RevisionTuple,
        scheduler_id: SchedulerId,
        root_task_id: CollaborationTaskId,
        limits: CollaborationLimits,
        root_assignment: Delegation,
        digest: Sha256Digest,
    ) -> Self {
        Self { id, run_id, revision, scheduler_id, root_task_id, limits, root_assignment, digest }
    }

    /// Returns the aggregate identity.
    #[must_use]
    pub const fn id(&self) -> CollaborationId {
        self.id
    }
    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the immutable revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the bound scheduler aggregate.
    #[must_use]
    pub const fn scheduler_id(&self) -> SchedulerId {
        self.scheduler_id
    }
    /// Returns the stable root task.
    #[must_use]
    pub const fn root_task_id(&self) -> CollaborationTaskId {
        self.root_task_id
    }
    /// Returns independently checked immutable limits.
    #[must_use]
    pub const fn limits(&self) -> CollaborationLimits {
        self.limits
    }
    /// Borrows the root assignment.
    #[must_use]
    pub const fn root_assignment(&self) -> &Delegation {
        &self.root_assignment
    }
    /// Returns the canonical complete binding digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(super) fn validate(&self) -> Result<(), CollaborationError> {
        if self.root_task_id != self.root_assignment.task_id()
            || self.root_assignment.root_task_id() != self.root_task_id
            || self.root_assignment.parent_task_id().is_some()
            || self.root_assignment.depth() != 0
            || !self.root_assignment.required()
            || crate::canonical::binding_digest(self) != self.digest
        {
            return Err(reject(
                CollaborationErrorKind::BindingMismatch,
                "decoded collaboration binding differs from its canonical root or digest",
            ));
        }
        Ok(())
    }
}
