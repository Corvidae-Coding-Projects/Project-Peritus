//! Verified exact clone of the actual command payload.

use vstd::prelude::*;

use super::SchedulerCommandKind;
#[cfg(verus_only)]
use crate::{SchedulerBinding, WorkSpec, WorkerDescriptor};

verus! {

impl SchedulerCommandKind {
    /// Relates an exact semantic clone of an actual scheduler command payload.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        match (left, right) {
            (Self::StartScheduler { binding: left }, Self::StartScheduler { binding: right }) => {
                SchedulerBinding::clone_equivalent(left, right)
            },
            (
                Self::RegisterWorker { descriptor: left },
                Self::RegisterWorker { descriptor: right },
            ) => WorkerDescriptor::clone_equivalent(left, right),
            (
                Self::SetWorkerAvailable { worker_id: left },
                Self::SetWorkerAvailable { worker_id: right },
            )
            | (Self::DrainWorker { worker_id: left }, Self::DrainWorker { worker_id: right })
            | (Self::LoseWorker { worker_id: left }, Self::LoseWorker { worker_id: right })
            | (Self::RemoveWorker { worker_id: left }, Self::RemoveWorker { worker_id: right }) => {
                left == right
            },
            (Self::AdmitWork { spec: left }, Self::AdmitWork { spec: right }) => {
                WorkSpec::clone_equivalent(left, right)
            },
            (
                Self::DispatchNext {
                    dispatch_id: left_id,
                    dispatch_token: left_token,
                },
                Self::DispatchNext {
                    dispatch_id: right_id,
                    dispatch_token: right_token,
                },
            ) => left_id == right_id && left_token == right_token,
            (
                Self::AcknowledgeStart { dispatch_id: left },
                Self::AcknowledgeStart { dispatch_id: right },
            ) => left == right,
            (
                Self::CompleteWork {
                    dispatch_id: left_id,
                    result_digest: left_digest,
                },
                Self::CompleteWork {
                    dispatch_id: right_id,
                    result_digest: right_digest,
                },
            ) => left_id == right_id && left_digest == right_digest,
            (
                Self::FailWork {
                    dispatch_id: left_id,
                    failure_digest: left_digest,
                    disposition: left_disposition,
                },
                Self::FailWork {
                    dispatch_id: right_id,
                    failure_digest: right_digest,
                    disposition: right_disposition,
                },
            ) => left_id == right_id
                && left_digest == right_digest
                && left_disposition == right_disposition,
            (Self::RetryWork { work_id: left }, Self::RetryWork { work_id: right })
            | (Self::CancelWork { work_id: left }, Self::CancelWork { work_id: right })
            | (Self::CancelWorkTree { work_id: left }, Self::CancelWorkTree { work_id: right }) => {
                left == right
            },
            (
                Self::AcknowledgeCancellation { dispatch_id: left },
                Self::AcknowledgeCancellation { dispatch_id: right },
            ) => left == right,
            (
                Self::ExhaustWork { work_id: left_id, cause_digest: left_digest },
                Self::ExhaustWork { work_id: right_id, cause_digest: right_digest },
            ) => left_id == right_id && left_digest == right_digest,
            (
                Self::AbandonDispatch { dispatch_id: left_id, cause_digest: left_digest },
                Self::AbandonDispatch { dispatch_id: right_id, cause_digest: right_digest },
            ) => left_id == right_id && left_digest == right_digest,
            (Self::PauseScheduler, Self::PauseScheduler)
            | (Self::ResumeScheduler, Self::ResumeScheduler)
            | (Self::DrainScheduler, Self::DrainScheduler)
            | (Self::FinalizeScheduler, Self::FinalizeScheduler) => true,
            _ => false,
        }
    }
}

impl Clone for SchedulerCommandKind {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        match self {
            Self::StartScheduler { binding } => {
                Self::StartScheduler { binding: binding.clone() }
            },
            Self::RegisterWorker { descriptor } => {
                Self::RegisterWorker { descriptor: descriptor.clone() }
            },
            Self::SetWorkerAvailable { worker_id } => {
                Self::SetWorkerAvailable { worker_id: *worker_id }
            },
            Self::DrainWorker { worker_id } => Self::DrainWorker { worker_id: *worker_id },
            Self::LoseWorker { worker_id } => Self::LoseWorker { worker_id: *worker_id },
            Self::RemoveWorker { worker_id } => Self::RemoveWorker { worker_id: *worker_id },
            Self::AdmitWork { spec } => Self::AdmitWork { spec: spec.clone() },
            Self::DispatchNext { dispatch_id, dispatch_token } => Self::DispatchNext {
                dispatch_id: *dispatch_id,
                dispatch_token: *dispatch_token,
            },
            Self::AcknowledgeStart { dispatch_id } => {
                Self::AcknowledgeStart { dispatch_id: *dispatch_id }
            },
            Self::CompleteWork { dispatch_id, result_digest } => Self::CompleteWork {
                dispatch_id: *dispatch_id,
                result_digest: *result_digest,
            },
            Self::FailWork { dispatch_id, failure_digest, disposition } => Self::FailWork {
                dispatch_id: *dispatch_id,
                failure_digest: *failure_digest,
                disposition: *disposition,
            },
            Self::RetryWork { work_id } => Self::RetryWork { work_id: *work_id },
            Self::CancelWork { work_id } => Self::CancelWork { work_id: *work_id },
            Self::CancelWorkTree { work_id } => Self::CancelWorkTree { work_id: *work_id },
            Self::AcknowledgeCancellation { dispatch_id } => {
                Self::AcknowledgeCancellation { dispatch_id: *dispatch_id }
            },
            Self::ExhaustWork { work_id, cause_digest } => Self::ExhaustWork {
                work_id: *work_id,
                cause_digest: *cause_digest,
            },
            Self::AbandonDispatch { dispatch_id, cause_digest } => Self::AbandonDispatch {
                dispatch_id: *dispatch_id,
                cause_digest: *cause_digest,
            },
            Self::PauseScheduler => Self::PauseScheduler,
            Self::ResumeScheduler => Self::ResumeScheduler,
            Self::DrainScheduler => Self::DrainScheduler,
            Self::FinalizeScheduler => Self::FinalizeScheduler,
        }
    }
}

} // verus!
