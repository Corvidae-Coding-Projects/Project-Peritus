//! Closed semantic facts emitted by the scheduler reducer.

use peritus_types::Sha256Digest;

use super::LossOutcome;
use crate::{
    DispatchId, FailureDisposition, SchedulerBinding, SchedulerReservation, SchedulerTerminal,
    WorkId, WorkSpec, WorkerDescriptor, WorkerId,
};

/// Closed semantic fact emitted by the scheduler reducer.
// Direct verification preserves the documented enum fields without relying on synthesized
// projections from the pinned Verus macro.

#[derive(Debug, Eq, PartialEq)]
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

impl SchedulerEventKind {}
fn clone_loss_outcomes(values: &[LossOutcome]) -> Vec<LossOutcome> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index].clone());
        index += 1;
    }
    result
}
fn clone_work_ids(values: &[WorkId]) -> Vec<WorkId> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index]);
        index += 1;
    }
    result
}
impl Clone for SchedulerEventKind {
    fn clone(&self) -> Self {
        match self {
            Self::SchedulerStarted { binding } => {
                Self::SchedulerStarted { binding: binding.clone() }
            }
            Self::WorkerRegistered { descriptor } => {
                Self::WorkerRegistered { descriptor: descriptor.clone() }
            }
            Self::WorkerAvailable { worker_id } => Self::WorkerAvailable { worker_id: *worker_id },
            Self::WorkerDrainRequested { worker_id } => {
                Self::WorkerDrainRequested { worker_id: *worker_id }
            }
            Self::WorkerLost { worker_id, outcomes } => {
                Self::WorkerLost { worker_id: *worker_id, outcomes: clone_loss_outcomes(outcomes) }
            }
            Self::WorkerRemoved { worker_id } => Self::WorkerRemoved { worker_id: *worker_id },
            Self::WorkAdmitted { spec } => Self::WorkAdmitted { spec: spec.clone() },
            Self::WorkReserved { reservation } => {
                Self::WorkReserved { reservation: reservation.clone() }
            }
            Self::WorkStartAcknowledged { dispatch_id } => {
                Self::WorkStartAcknowledged { dispatch_id: *dispatch_id }
            }
            Self::WorkSucceeded { dispatch_id, result_digest } => {
                Self::WorkSucceeded { dispatch_id: *dispatch_id, result_digest: *result_digest }
            }
            Self::WorkFailed { dispatch_id, failure_digest, disposition } => Self::WorkFailed {
                dispatch_id: *dispatch_id,
                failure_digest: *failure_digest,
                disposition: *disposition,
            },
            Self::WorkRetryQueued { work_id } => Self::WorkRetryQueued { work_id: *work_id },
            Self::WorkCancelled { work_id, descendants, affected } => Self::WorkCancelled {
                work_id: *work_id,
                descendants: *descendants,
                affected: clone_work_ids(affected),
            },
            Self::CancellationAcknowledged { dispatch_id } => {
                Self::CancellationAcknowledged { dispatch_id: *dispatch_id }
            }
            Self::WorkExhausted { work_id, cause_digest } => {
                Self::WorkExhausted { work_id: *work_id, cause_digest: *cause_digest }
            }
            Self::DispatchAbandoned { dispatch_id, cause_digest } => {
                Self::DispatchAbandoned { dispatch_id: *dispatch_id, cause_digest: *cause_digest }
            }
            Self::SchedulerPaused => Self::SchedulerPaused,
            Self::SchedulerResumed => Self::SchedulerResumed,
            Self::SchedulerDrainRequested => Self::SchedulerDrainRequested,
            Self::SchedulerFinalized { terminal } => {
                Self::SchedulerFinalized { terminal: terminal.clone() }
            }
        }
    }
}
