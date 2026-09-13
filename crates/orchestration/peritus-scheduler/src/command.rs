//! Closed pure scheduler command vocabulary.

use peritus_types::{CommandId, EventId, RevisionTuple, RunId, Sha256Digest};

use crate::{
    DispatchId, SchedulerBinding, SchedulerError, SchedulerErrorKind, WorkId, WorkSpec,
    WorkerDescriptor, WorkerId,
};

/// Caller-selected classification of an observed execution failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureDisposition {
    /// Release ownership and await explicit bounded retry.
    Retryable,
    /// Release ownership and retain terminal failure.
    Failed,
    /// Release ownership and retain unknowable external outcome.
    Ambiguous,
}

/// Complete semantic payload of one fenced scheduler command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulerCommandKind {
    /// Creates one immutable scheduler aggregate.
    StartScheduler {
        /// Immutable scheduler binding.
        binding: SchedulerBinding,
    },
    /// Registers one checked worker.
    RegisterWorker {
        /// Checked worker definition.
        descriptor: WorkerDescriptor,
    },
    /// Makes one quiescent draining or lost worker available again.
    SetWorkerAvailable {
        /// Worker becoming available.
        worker_id: WorkerId,
    },
    /// Prevents new reservations while preserving current ownership.
    DrainWorker {
        /// Worker entering drain mode.
        worker_id: WorkerId,
    },
    /// Classifies and releases every reservation owned by a lost worker.
    LoseWorker {
        /// Worker whose ownership was lost.
        worker_id: WorkerId,
    },
    /// Permanently removes one quiescent worker.
    RemoveWorker {
        /// Quiescent worker to remove.
        worker_id: WorkerId,
    },
    /// Admits one complete immutable work specification.
    AdmitWork {
        /// Immutable work definition.
        spec: WorkSpec,
    },
    /// Deterministically reserves the next feasible item.
    DispatchNext {
        /// Identity reserved for the selected dispatch.
        dispatch_id: DispatchId,
        /// Idempotent effect-delivery token digest.
        dispatch_token: Sha256Digest,
    },
    /// Records owner acknowledgement of one committed dispatch.
    AcknowledgeStart {
        /// Dispatch acknowledged by its worker.
        dispatch_id: DispatchId,
    },
    /// Releases reservation and retains success.
    CompleteWork {
        /// Dispatch completing successfully.
        dispatch_id: DispatchId,
        /// Digest of the inert result.
        result_digest: Sha256Digest,
    },
    /// Releases reservation and retains caller-classified failure.
    FailWork {
        /// Dispatch reporting failure.
        dispatch_id: DispatchId,
        /// Digest of the inert failure record.
        failure_digest: Sha256Digest,
        /// Explicit retry or terminal classification.
        disposition: FailureDisposition,
    },
    /// Moves one retry-pending item back to deterministic queue selection.
    RetryWork {
        /// Retry-pending work to requeue.
        work_id: WorkId,
    },
    /// Cancels one item without traversing descendants.
    CancelWork {
        /// Work item to cancel.
        work_id: WorkId,
    },
    /// Cancels one item and every retained work descendant.
    CancelWorkTree {
        /// Root of the cancellation subtree.
        work_id: WorkId,
    },
    /// Records owner termination acknowledgement and releases a cancelling reservation.
    AcknowledgeCancellation {
        /// Cancelling dispatch acknowledged by its owner.
        dispatch_id: DispatchId,
    },
    /// Explicitly exhausts one inactive item.
    ExhaustWork {
        /// Inactive work to exhaust.
        work_id: WorkId,
        /// Digest explaining exhaustion.
        cause_digest: Sha256Digest,
    },
    /// Releases and truthfully abandons one active reservation.
    AbandonDispatch {
        /// Active dispatch to abandon.
        dispatch_id: DispatchId,
        /// Digest explaining abandonment.
        cause_digest: Sha256Digest,
    },
    /// Prevents new dispatch without disturbing admission or ownership.
    PauseScheduler,
    /// Restores dispatch after pause.
    ResumeScheduler,
    /// Closes new admission while allowing retained queued work to drain.
    DrainScheduler,
    /// Commits terminal truth after all work and directives quiesce.
    FinalizeScheduler,
}

/// Syntax-checked but unprivileged scheduler reducer command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerCommand {
    command_id: CommandId,
    event_id: EventId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: SchedulerCommandKind,
}

impl SchedulerCommand {
    /// Creates one exact fenced command.
    ///
    /// # Errors
    /// Rejects inconsistent genesis/non-genesis predecessor shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: SchedulerCommandKind,
    ) -> Result<Self, SchedulerError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(crate::error::reject(
                SchedulerErrorKind::StaleFence,
                "scheduler command predecessor shape is inconsistent",
            ));
        }
        Ok(Self::from_wire(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: SchedulerCommandKind,
    ) -> Self {
        Self {
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        }
    }
    /// Returns idempotent command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns expected prior sequence, zero at genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns exact prior event identity.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns exact predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns exact immutable revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows closed semantic payload.
    #[must_use]
    pub const fn kind(&self) -> &SchedulerCommandKind {
        &self.kind
    }
}
