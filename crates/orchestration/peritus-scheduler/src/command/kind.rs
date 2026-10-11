//! Actual semantic scheduler command payload.

use peritus_types::Sha256Digest;

use crate::{DispatchId, SchedulerBinding, WorkId, WorkSpec, WorkerDescriptor, WorkerId};

mod clone_impl;

#[doc = " Caller-selected classification of an observed execution failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureDisposition {
    #[doc = " Release ownership and await explicit bounded retry."]
    Retryable,
    #[doc = " Release ownership and retain terminal failure."]
    Failed,
    #[doc = " Release ownership and retain unknowable external outcome."]
    Ambiguous,
}
#[doc = " Complete semantic payload of one fenced scheduler command."]
#[derive(Debug, Eq, PartialEq)]
pub enum SchedulerCommandKind {
    #[doc = " Creates one immutable scheduler aggregate."]
    StartScheduler {
        #[doc = " Immutable scheduler binding."]
        binding: SchedulerBinding,
    },
    #[doc = " Registers one checked worker."]
    RegisterWorker {
        #[doc = " Checked worker definition."]
        descriptor: WorkerDescriptor,
    },
    #[doc = " Makes one quiescent draining or lost worker available again."]
    SetWorkerAvailable {
        #[doc = " Worker becoming available."]
        worker_id: WorkerId,
    },
    #[doc = " Prevents new reservations while preserving current ownership."]
    DrainWorker {
        #[doc = " Worker entering drain mode."]
        worker_id: WorkerId,
    },
    #[doc = " Classifies and releases every reservation owned by a lost worker."]
    LoseWorker {
        #[doc = " Worker whose ownership was lost."]
        worker_id: WorkerId,
    },
    #[doc = " Permanently removes one quiescent worker."]
    RemoveWorker {
        #[doc = " Quiescent worker to remove."]
        worker_id: WorkerId,
    },
    #[doc = " Admits one complete immutable work specification."]
    AdmitWork {
        #[doc = " Immutable work definition."]
        spec: WorkSpec,
    },
    #[doc = " Deterministically reserves the next feasible item."]
    DispatchNext {
        #[doc = " Identity reserved for the selected dispatch."]
        dispatch_id: DispatchId,
        #[doc = " Idempotent effect-delivery token digest."]
        dispatch_token: Sha256Digest,
    },
    #[doc = " Records owner acknowledgement of one committed dispatch."]
    AcknowledgeStart {
        #[doc = " Dispatch acknowledged by its worker."]
        dispatch_id: DispatchId,
    },
    #[doc = " Releases reservation and retains success."]
    CompleteWork {
        #[doc = " Dispatch completing successfully."]
        dispatch_id: DispatchId,
        #[doc = " Digest of the inert result."]
        result_digest: Sha256Digest,
    },
    #[doc = " Releases reservation and retains caller-classified failure."]
    FailWork {
        #[doc = " Dispatch reporting failure."]
        dispatch_id: DispatchId,
        #[doc = " Digest of the inert failure record."]
        failure_digest: Sha256Digest,
        #[doc = " Explicit retry or terminal classification."]
        disposition: FailureDisposition,
    },
    #[doc = " Moves one retry-pending item back to deterministic queue selection."]
    RetryWork {
        #[doc = " Retry-pending work to requeue."]
        work_id: WorkId,
    },
    #[doc = " Cancels one item without traversing descendants."]
    CancelWork {
        #[doc = " Work item to cancel."]
        work_id: WorkId,
    },
    #[doc = " Cancels one item and every retained work descendant."]
    CancelWorkTree {
        #[doc = " Root of the cancellation subtree."]
        work_id: WorkId,
    },
    #[doc = " Records owner termination acknowledgement and releases a cancelling reservation."]
    AcknowledgeCancellation {
        #[doc = " Cancelling dispatch acknowledged by its owner."]
        dispatch_id: DispatchId,
    },
    #[doc = " Explicitly exhausts one inactive item."]
    ExhaustWork {
        #[doc = " Inactive work to exhaust."]
        work_id: WorkId,
        #[doc = " Digest explaining exhaustion."]
        cause_digest: Sha256Digest,
    },
    #[doc = " Releases and truthfully abandons one active reservation."]
    AbandonDispatch {
        #[doc = " Active dispatch to abandon."]
        dispatch_id: DispatchId,
        #[doc = " Digest explaining abandonment."]
        cause_digest: Sha256Digest,
    },
    #[doc = " Prevents new dispatch without disturbing admission or ownership."]
    PauseScheduler,
    #[doc = " Restores dispatch after pause."]
    ResumeScheduler,
    #[doc = " Closes new admission while allowing retained queued work to drain."]
    DrainScheduler,
    #[doc = " Commits terminal truth after all work and directives quiesce."]
    FinalizeScheduler,
}
