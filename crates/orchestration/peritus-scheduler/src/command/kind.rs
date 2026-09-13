//! Actual semantic scheduler command payload.

#![cfg_attr(
    verus_keep_ghost,
    allow(
        missing_docs,
        reason = "pinned Verus enum synthesis discards documented field projection metadata"
    )
)]

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{DispatchId, SchedulerBinding, WorkId, WorkSpec, WorkerDescriptor, WorkerId};

mod clone_impl;

verus! {

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
#[derive(Debug, Eq, PartialEq)]
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

} // verus!
