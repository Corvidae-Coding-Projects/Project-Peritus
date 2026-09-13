//! Immutable work specifications and authoritative lifecycle records.

use peritus_types::{ActorId, BudgetReservationId, RevisionTuple, Sha256Digest};

use crate::{
    AttemptNumber, DispatchId, ResourceVector, SchedulerError, SchedulerErrorKind, SchedulerLimits,
    WorkId,
};

/// Closed supported execution class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionClass {
    /// Model inference or agent turn.
    Model,
    /// Inert tool request handled by a governed router.
    Tool,
    /// Acceptance-gate execution.
    Gate,
    /// Independent review work.
    Review,
    /// Orchestration/control-plane work.
    Coordination,
}

/// Worker-loss classification fixed at admission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryPolicy {
    /// Ownership loss safely requeues under the next attempt.
    RetrySafe,
    /// Ownership loss has ambiguous external outcome and cannot be retried automatically.
    Ambiguous,
    /// Ownership loss is a terminal failure.
    Fail,
}

/// Immutable admitted work definition.
#[derive(Clone, Debug, Eq, PartialEq)]
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

    /// Returns stable work identity.
    #[must_use]
    pub const fn id(&self) -> WorkId {
        self.id
    }
    /// Returns assigned owning actor.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Returns exact immutable revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns required execution class.
    #[must_use]
    pub const fn class(&self) -> ExecutionClass {
        self.class
    }
    /// Returns priority, where larger values precede smaller values.
    #[must_use]
    pub const fn priority(&self) -> u8 {
        self.priority
    }
    /// Borrows exact resource request.
    #[must_use]
    pub const fn request(&self) -> &ResourceVector {
        &self.request
    }
    /// Returns optional observed B1 reservation reference without authority over it.
    #[must_use]
    pub const fn budget_reservation(&self) -> Option<BudgetReservationId> {
        self.budget_reservation
    }
    /// Borrows canonical dependencies.
    #[must_use]
    pub fn dependencies(&self) -> &[WorkId] {
        &self.dependencies
    }
    /// Returns optional parent work.
    #[must_use]
    pub const fn parent(&self) -> Option<WorkId> {
        self.parent
    }
    /// Returns maximum allowed attempts.
    #[must_use]
    pub const fn maximum_attempts(&self) -> AttemptNumber {
        self.maximum_attempts
    }
    /// Returns fixed worker-loss policy.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryPolicy {
        self.recovery
    }
    /// Returns inert exact payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
}

/// Closed work lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkPhase {
    /// Waiting for successful dependencies.
    WaitingDependencies,
    /// Eligible for deterministic selection.
    Queued,
    /// Reserved durably but not yet acknowledged by the worker.
    Reserved,
    /// Worker acknowledged execution ownership.
    Running,
    /// Retryable failure awaits explicit retry command.
    RetryPending,
    /// Cancellation was requested and awaits owner acknowledgement.
    Cancelling,
    /// Immutable non-running outcome is retained.
    Terminal,
}

/// Truthful terminal work outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkTerminal {
    /// Work completed with exact inert result digest.
    Succeeded {
        /// Digest of the inert result.
        result_digest: Sha256Digest,
    },
    /// Work failed with exact inert failure digest.
    Failed {
        /// Digest of the inert failure record.
        failure_digest: Sha256Digest,
    },
    /// A canonical prerequisite could not succeed.
    DependencyFailed {
        /// Canonical prerequisite that could not succeed.
        dependency: WorkId,
    },
    /// Work was cancelled before or during execution.
    Cancelled,
    /// Lost ownership left an unknowable external outcome.
    Ambiguous {
        /// Dispatch whose external outcome is unknowable.
        dispatch_id: DispatchId,
    },
    /// Bounded attempts were exhausted.
    Exhausted {
        /// Digest explaining attempt exhaustion.
        cause_digest: Sha256Digest,
    },
    /// An active reservation was explicitly abandoned.
    Abandoned {
        /// Digest explaining explicit abandonment.
        cause_digest: Sha256Digest,
    },
}

/// Retained work plus deterministic queue/attempt accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkRecord {
    spec: WorkSpec,
    phase: WorkPhase,
    enqueue_ordinal: u64,
    bypasses: u16,
    attempts_started: u16,
    retry_cause: Option<Sha256Digest>,
    terminal: Option<WorkTerminal>,
}

impl WorkRecord {
    pub(crate) const fn new(spec: WorkSpec, phase: WorkPhase, enqueue_ordinal: u64) -> Self {
        Self {
            spec,
            phase,
            enqueue_ordinal,
            bypasses: 0,
            attempts_started: 0,
            retry_cause: None,
            terminal: None,
        }
    }
    /// Borrows immutable work definition.
    #[must_use]
    pub const fn spec(&self) -> &WorkSpec {
        &self.spec
    }
    /// Returns current lifecycle.
    #[must_use]
    pub const fn phase(&self) -> WorkPhase {
        self.phase
    }
    /// Returns immutable admission order.
    #[must_use]
    pub const fn enqueue_ordinal(&self) -> u64 {
        self.enqueue_ordinal
    }
    /// Returns bounded feasible-bypass count.
    #[must_use]
    pub const fn bypasses(&self) -> u16 {
        self.bypasses
    }
    /// Returns attempts reserved so far.
    #[must_use]
    pub const fn attempts_started(&self) -> u16 {
        self.attempts_started
    }
    /// Returns latest retryable failure cause.
    #[must_use]
    pub const fn retry_cause(&self) -> Option<Sha256Digest> {
        self.retry_cause
    }
    /// Borrows terminal outcome.
    #[must_use]
    pub const fn terminal(&self) -> Option<&WorkTerminal> {
        self.terminal.as_ref()
    }

    pub(crate) const fn set_phase(&mut self, phase: WorkPhase) {
        self.phase = phase;
    }
    pub(crate) const fn set_bypasses(&mut self, value: u16) {
        self.bypasses = value;
    }
    pub(crate) fn begin_attempt(&mut self) -> Result<AttemptNumber, SchedulerError> {
        let next = self.attempts_started.checked_add(1).ok_or_else(|| {
            crate::error::reject(SchedulerErrorKind::LimitExceeded, "work attempt count overflowed")
        })?;
        if next > self.spec.maximum_attempts().get() {
            return Err(crate::error::reject(
                SchedulerErrorKind::LimitExceeded,
                "work attempt bound is exhausted",
            ));
        }
        self.attempts_started = next;
        self.phase = WorkPhase::Reserved;
        self.retry_cause = None;
        Ok(AttemptNumber::from_wire(next))
    }
    pub(crate) const fn set_retry_pending(&mut self, cause: Sha256Digest) {
        self.phase = WorkPhase::RetryPending;
        self.retry_cause = Some(cause);
    }
    pub(crate) const fn queue_retry(&mut self) {
        self.phase = WorkPhase::Queued;
        self.retry_cause = None;
    }
    pub(crate) const fn terminalize(&mut self, terminal: WorkTerminal) {
        self.phase = WorkPhase::Terminal;
        self.retry_cause = None;
        self.terminal = Some(terminal);
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "exact closed-wire work record fields are reconstructed without defaults"
    )]
    pub(crate) const fn from_wire(
        spec: WorkSpec,
        phase: WorkPhase,
        enqueue_ordinal: u64,
        bypasses: u16,
        attempts_started: u16,
        retry_cause: Option<Sha256Digest>,
        terminal: Option<WorkTerminal>,
    ) -> Self {
        Self { spec, phase, enqueue_ordinal, bypasses, attempts_started, retry_cause, terminal }
    }
}
