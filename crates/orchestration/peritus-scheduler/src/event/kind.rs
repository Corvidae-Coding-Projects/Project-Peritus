//! Closed semantic facts emitted by the scheduler reducer.

use peritus_types::Sha256Digest;

use super::LossOutcome;
use crate::{
    DispatchId, FailureDisposition, SchedulerBinding, SchedulerReservation, SchedulerTerminal,
    WorkId, WorkSpec, WorkerDescriptor, WorkerId,
};
use vstd::prelude::*;

/// Closed semantic fact emitted by the scheduler reducer.
// Direct verification preserves the documented enum fields without relying on synthesized
// projections from the pinned Verus macro.
#[cfg_attr(verus_keep_ghost, verifier::verify)]
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

verus! {

impl SchedulerEventKind {
    /// Relates an exact semantic clone of an accepted scheduler fact.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        match (left, right) {
            (Self::SchedulerStarted { binding: left }, Self::SchedulerStarted { binding: right }) => {
                SchedulerBinding::clone_equivalent(left, right)
            },
            (
                Self::WorkerRegistered { descriptor: left },
                Self::WorkerRegistered { descriptor: right },
            ) => WorkerDescriptor::clone_equivalent(left, right),
            (
                Self::WorkerAvailable { worker_id: left },
                Self::WorkerAvailable { worker_id: right },
            )
            | (
                Self::WorkerDrainRequested { worker_id: left },
                Self::WorkerDrainRequested { worker_id: right },
            )
            | (Self::WorkerRemoved { worker_id: left }, Self::WorkerRemoved { worker_id: right }) => {
                left == right
            },
            (
                Self::WorkerLost { worker_id: left_id, outcomes: left_outcomes },
                Self::WorkerLost { worker_id: right_id, outcomes: right_outcomes },
            ) => left_id == right_id && left_outcomes@ == right_outcomes@,
            (Self::WorkAdmitted { spec: left }, Self::WorkAdmitted { spec: right }) => {
                WorkSpec::clone_equivalent(left, right)
            },
            (
                Self::WorkReserved { reservation: left },
                Self::WorkReserved { reservation: right },
            ) => SchedulerReservation::clone_equivalent(left, right),
            (
                Self::WorkStartAcknowledged { dispatch_id: left },
                Self::WorkStartAcknowledged { dispatch_id: right },
            )
            | (
                Self::CancellationAcknowledged { dispatch_id: left },
                Self::CancellationAcknowledged { dispatch_id: right },
            ) => left == right,
            (
                Self::WorkSucceeded { dispatch_id: left_id, result_digest: left_digest },
                Self::WorkSucceeded { dispatch_id: right_id, result_digest: right_digest },
            ) => left_id == right_id && left_digest == right_digest,
            (
                Self::WorkFailed {
                    dispatch_id: left_id,
                    failure_digest: left_digest,
                    disposition: left_disposition,
                },
                Self::WorkFailed {
                    dispatch_id: right_id,
                    failure_digest: right_digest,
                    disposition: right_disposition,
                },
            ) => left_id == right_id
                && left_digest == right_digest
                && left_disposition == right_disposition,
            (Self::WorkRetryQueued { work_id: left }, Self::WorkRetryQueued { work_id: right }) => {
                left == right
            },
            (
                Self::WorkCancelled {
                    work_id: left_id,
                    descendants: left_descendants,
                    affected: left_affected,
                },
                Self::WorkCancelled {
                    work_id: right_id,
                    descendants: right_descendants,
                    affected: right_affected,
                },
            ) => left_id == right_id
                && left_descendants == right_descendants
                && left_affected@ == right_affected@,
            (
                Self::WorkExhausted { work_id: left_id, cause_digest: left_digest },
                Self::WorkExhausted { work_id: right_id, cause_digest: right_digest },
            ) => left_id == right_id && left_digest == right_digest,
            (
                Self::DispatchAbandoned { dispatch_id: left_id, cause_digest: left_digest },
                Self::DispatchAbandoned { dispatch_id: right_id, cause_digest: right_digest },
            ) => left_id == right_id && left_digest == right_digest,
            (Self::SchedulerPaused, Self::SchedulerPaused)
            | (Self::SchedulerResumed, Self::SchedulerResumed)
            | (Self::SchedulerDrainRequested, Self::SchedulerDrainRequested) => true,
            (
                Self::SchedulerFinalized { terminal: left },
                Self::SchedulerFinalized { terminal: right },
            ) => SchedulerTerminal::clone_equivalent(left, right),
            _ => false,
        }
    }
}

fn clone_loss_outcomes(values: &[LossOutcome]) -> (result: Vec<LossOutcome>)
    ensures result@ == values@,
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            result@ == values@.take(index as int),
        decreases values@.len() - index,
    {
        result.push(values[index].clone());
        index += 1;
    }
    result
}

fn clone_work_ids(values: &[WorkId]) -> (result: Vec<WorkId>)
    ensures result@ == values@,
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            result@ == values@.take(index as int),
        decreases values@.len() - index,
    {
        result.push(values[index]);
        index += 1;
    }
    result
}

impl Clone for SchedulerEventKind {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        match self {
            Self::SchedulerStarted { binding } => {
                Self::SchedulerStarted { binding: binding.clone() }
            },
            Self::WorkerRegistered { descriptor } => {
                Self::WorkerRegistered { descriptor: descriptor.clone() }
            },
            Self::WorkerAvailable { worker_id } => Self::WorkerAvailable { worker_id: *worker_id },
            Self::WorkerDrainRequested { worker_id } => {
                Self::WorkerDrainRequested { worker_id: *worker_id }
            },
            Self::WorkerLost { worker_id, outcomes } => Self::WorkerLost {
                worker_id: *worker_id,
                outcomes: clone_loss_outcomes(outcomes),
            },
            Self::WorkerRemoved { worker_id } => Self::WorkerRemoved { worker_id: *worker_id },
            Self::WorkAdmitted { spec } => Self::WorkAdmitted { spec: spec.clone() },
            Self::WorkReserved { reservation } => {
                Self::WorkReserved { reservation: reservation.clone() }
            },
            Self::WorkStartAcknowledged { dispatch_id } => {
                Self::WorkStartAcknowledged { dispatch_id: *dispatch_id }
            },
            Self::WorkSucceeded { dispatch_id, result_digest } => Self::WorkSucceeded {
                dispatch_id: *dispatch_id,
                result_digest: *result_digest,
            },
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
            },
            Self::WorkExhausted { work_id, cause_digest } => Self::WorkExhausted {
                work_id: *work_id,
                cause_digest: *cause_digest,
            },
            Self::DispatchAbandoned { dispatch_id, cause_digest } => Self::DispatchAbandoned {
                dispatch_id: *dispatch_id,
                cause_digest: *cause_digest,
            },
            Self::SchedulerPaused => Self::SchedulerPaused,
            Self::SchedulerResumed => Self::SchedulerResumed,
            Self::SchedulerDrainRequested => Self::SchedulerDrainRequested,
            Self::SchedulerFinalized { terminal } => {
                Self::SchedulerFinalized { terminal: terminal.clone() }
            },
        }
    }
}

} // verus!
