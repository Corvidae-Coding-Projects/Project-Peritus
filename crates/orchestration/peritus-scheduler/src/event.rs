//! Immutable scheduler facts and successor transitions.

use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, RunId, Sha256Digest};

use crate::{
    DispatchId, FailureDisposition, SchedulerBinding, SchedulerReservation, SchedulerState,
    SchedulerTerminal, WorkId, WorkSpec, WorkerDescriptor, WorkerId,
};

/// Deterministic work outcome caused by one worker-loss event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LossOutcome {
    /// Safe ownership loss released and requeued the work.
    Requeued {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work returned to the queue.
        work_id: WorkId,
    },
    /// Attempt bound was reached during safe-retry classification.
    Exhausted {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work whose attempt bound was reached.
        work_id: WorkId,
    },
    /// External result became ambiguous.
    Ambiguous {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work with unknowable external outcome.
        work_id: WorkId,
    },
    /// Loss policy classified work as failed.
    Failed {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work classified as failed.
        work_id: WorkId,
    },
    /// Cancellation already dominated the lost ownership.
    Cancelled {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work whose cancellation dominated the loss.
        work_id: WorkId,
    },
}

/// Closed semantic fact emitted by the scheduler reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulerEventKind {
    /// Scheduler aggregate started.
    SchedulerStarted {
        /// Immutable scheduler binding.
        binding: SchedulerBinding,
    },
    /// Worker was registered.
    WorkerRegistered {
        /// Registered worker definition.
        descriptor: WorkerDescriptor,
    },
    /// Worker became available.
    WorkerAvailable {
        /// Worker made available.
        worker_id: WorkerId,
    },
    /// Worker began draining.
    WorkerDrainRequested {
        /// Worker entering drain mode.
        worker_id: WorkerId,
    },
    /// Worker was lost and all owned dispatches were classified.
    WorkerLost {
        /// Lost worker identity.
        worker_id: WorkerId,
        /// Canonical outcomes for released ownership.
        outcomes: Vec<LossOutcome>,
    },
    /// Quiescent worker was removed.
    WorkerRemoved {
        /// Removed worker identity.
        worker_id: WorkerId,
    },
    /// Work was durably admitted.
    WorkAdmitted {
        /// Immutable admitted work definition.
        spec: WorkSpec,
    },
    /// Exact dispatch was reserved before effect delivery.
    WorkReserved {
        /// Durable dispatch ownership reservation.
        reservation: SchedulerReservation,
    },
    /// Worker acknowledged execution ownership.
    WorkStartAcknowledged {
        /// Acknowledged dispatch identity.
        dispatch_id: DispatchId,
    },
    /// Work succeeded and ownership was released.
    WorkSucceeded {
        /// Successful dispatch identity.
        dispatch_id: DispatchId,
        /// Digest of the inert result.
        result_digest: Sha256Digest,
    },
    /// Work failed and ownership was released under an explicit disposition.
    WorkFailed {
        /// Failed dispatch identity.
        dispatch_id: DispatchId,
        /// Digest of the inert failure record.
        failure_digest: Sha256Digest,
        /// Applied retry or terminal classification.
        disposition: FailureDisposition,
    },
    /// Retry-pending work returned to the queue.
    WorkRetryQueued {
        /// Work returned to the queue.
        work_id: WorkId,
    },
    /// Work cancellation was applied to a canonical affected set.
    WorkCancelled {
        /// Root cancellation target.
        work_id: WorkId,
        /// Whether descendants were traversed.
        descendants: bool,
        /// Canonically ordered affected work identities.
        affected: Vec<WorkId>,
    },
    /// Active cancellation was acknowledged and released.
    CancellationAcknowledged {
        /// Released cancelling dispatch identity.
        dispatch_id: DispatchId,
    },
    /// Inactive work was explicitly exhausted.
    WorkExhausted {
        /// Exhausted work identity.
        work_id: WorkId,
        /// Digest explaining exhaustion.
        cause_digest: Sha256Digest,
    },
    /// Active ownership was explicitly abandoned.
    DispatchAbandoned {
        /// Abandoned dispatch identity.
        dispatch_id: DispatchId,
        /// Digest explaining abandonment.
        cause_digest: Sha256Digest,
    },
    /// Scheduler dispatch was paused.
    SchedulerPaused,
    /// Scheduler dispatch resumed.
    SchedulerResumed,
    /// Scheduler closed admission and began draining.
    SchedulerDrainRequested,
    /// Truthful terminal summary was committed.
    SchedulerFinalized {
        /// Truthful immutable final summary.
        terminal: SchedulerTerminal,
    },
}

/// One canonical event carrying all predecessor/successor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerEvent {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: SchedulerEventKind,
}

impl SchedulerEvent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: SchedulerEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            sequence,
            previous_event,
            run_id,
            revision,
            prior_state_digest,
            successor_state_digest,
            kind,
        }
    }
    /// Returns event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Returns causative command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns one-based aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns exact causal predecessor.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns exact immutable revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Borrows accepted semantic fact.
    #[must_use]
    pub const fn kind(&self) -> &SchedulerEventKind {
        &self.kind
    }
}

/// Pure accepted event plus complete successor checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerTransition {
    event: SchedulerEvent,
    state: SchedulerState,
}

impl SchedulerTransition {
    pub(crate) const fn new(event: SchedulerEvent, state: SchedulerState) -> Self {
        Self { event, state }
    }
    /// Borrows immutable event.
    #[must_use]
    pub const fn event(&self) -> &SchedulerEvent {
        &self.event
    }
    /// Borrows complete successor state.
    #[must_use]
    pub const fn state(&self) -> &SchedulerState {
        &self.state
    }
    /// Consumes transition after durable commit.
    #[must_use]
    pub fn into_state(self) -> SchedulerState {
        self.state
    }
}
