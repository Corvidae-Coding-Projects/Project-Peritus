//! Immutable work specifications and authoritative lifecycle records.

use peritus_types::{ActorId, BudgetReservationId, RevisionTuple, Sha256Digest};

use crate::{
    AttemptNumber, ResourceVector, SchedulerError, SchedulerErrorKind, SchedulerLimits, WorkId,
};

mod class;
mod clone_impl;
mod phase;
mod record;
mod terminal;

pub use class::ExecutionClass;
pub use phase::{RecoveryPolicy, WorkPhase};
pub use terminal::WorkTerminal;

#[doc = " Immutable admitted work definition."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkSpec {
    id: WorkId,
    owner: ActorId,
    revision: RevisionTuple,
    class: ExecutionClass,
    priority: u8,
    request: ResourceVector,
    budget_reservation: Option<BudgetReservationId>,
    dependencies: Vec<WorkId>,
    parent: Option<WorkId>,
    maximum_attempts: AttemptNumber,
    recovery: RecoveryPolicy,
    payload_digest: Sha256Digest,
}
impl WorkSpec {
    #[doc = " Borrows canonical dependencies."]
    #[must_use]
    pub fn dependencies(&self) -> &[WorkId] {
        &self.dependencies
    }
    #[doc = " Returns optional parent work."]
    #[must_use]
    pub const fn parent(&self) -> Option<WorkId> {
        self.parent
    }
    #[doc = " Returns stable work identity."]
    #[must_use]
    pub const fn id(&self) -> WorkId {
        self.id
    }
    #[doc = " Returns assigned owning actor."]
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    #[doc = " Returns required execution class."]
    #[must_use]
    pub const fn class(&self) -> ExecutionClass {
        self.class
    }
    #[doc = " Returns priority, where larger values precede smaller values."]
    #[must_use]
    pub const fn priority(&self) -> u8 {
        self.priority
    }
    #[doc = " Borrows exact resource request."]
    #[must_use]
    pub const fn request(&self) -> &ResourceVector {
        &self.request
    }
    #[doc = " Returns maximum allowed attempts."]
    #[must_use]
    pub const fn maximum_attempts(&self) -> AttemptNumber {
        self.maximum_attempts
    }
    #[doc = " Returns recorded worker-loss recovery policy."]
    #[must_use]
    pub const fn recovery(&self) -> RecoveryPolicy {
        self.recovery
    }
    #[doc = " Returns exact immutable revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
}

impl WorkSpec {
    /// Creates checked inert work.
    ///
    /// # Errors
    /// Rejects self references, noncanonical dependencies, resource overflow, or attempt excess.
    #[allow(clippy::too_many_arguments, reason = "immutable admission fields stay explicit")]
    pub fn new(
        id: WorkId,
        owner: ActorId,
        revision: RevisionTuple,
        class: ExecutionClass,
        priority: u8,
        request: ResourceVector,
        budget_reservation: Option<BudgetReservationId>,
        dependencies: Vec<WorkId>,
        parent: Option<WorkId>,
        maximum_attempts: AttemptNumber,
        recovery: RecoveryPolicy,
        payload_digest: Sha256Digest,
        limits: SchedulerLimits,
    ) -> Result<Self, SchedulerError> {
        if dependencies.len() > usize::from(limits.dependencies_per_work())
            || dependencies.windows(2).any(|pair| pair[0] >= pair[1])
            || dependencies.binary_search(&id).is_ok()
            || parent == Some(id)
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::NonCanonical,
                "work dependencies are oversized, duplicated, unsorted, or self-referential",
            ));
        }
        if maximum_attempts.get() > limits.attempts_per_work() {
            return Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "work attempt bound exceeds scheduler limit",
            ));
        }
        request.validate(limits.resource_dimensions())?;
        Ok(Self {
            id,
            owner,
            revision,
            class,
            priority,
            request,
            budget_reservation,
            dependencies,
            parent,
            maximum_attempts,
            recovery,
            payload_digest,
        })
    }

    /// Returns optional observed B1 reservation reference without authority over it.
    #[must_use]
    pub const fn budget_reservation(&self) -> Option<BudgetReservationId> {
        self.budget_reservation
    }
    /// Returns inert exact payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
}

#[doc = " Retained work plus deterministic queue/attempt accounting."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkRecord {
    spec: WorkSpec,
    phase: WorkPhase,
    enqueue_ordinal: u64,
    bypasses: u16,
    attempts_started: u16,
    retry_cause: Option<Sha256Digest>,
    terminal: Option<WorkTerminal>,
}
