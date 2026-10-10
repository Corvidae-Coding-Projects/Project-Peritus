//! Verified exact clone of the actual command payload.

use super::SchedulerCommandKind;

impl SchedulerCommandKind {}
impl Clone for SchedulerCommandKind {
    fn clone(&self) -> Self {
        match self {
            Self::StartScheduler { binding } => Self::StartScheduler { binding: binding.clone() },
            Self::RegisterWorker { descriptor } => {
                Self::RegisterWorker { descriptor: descriptor.clone() }
            }
            Self::SetWorkerAvailable { worker_id } => {
                Self::SetWorkerAvailable { worker_id: *worker_id }
            }
            Self::DrainWorker { worker_id } => Self::DrainWorker { worker_id: *worker_id },
            Self::LoseWorker { worker_id } => Self::LoseWorker { worker_id: *worker_id },
            Self::RemoveWorker { worker_id } => Self::RemoveWorker { worker_id: *worker_id },
            Self::AdmitWork { spec } => Self::AdmitWork { spec: spec.clone() },
            Self::DispatchNext { dispatch_id, dispatch_token } => {
                Self::DispatchNext { dispatch_id: *dispatch_id, dispatch_token: *dispatch_token }
            }
            Self::AcknowledgeStart { dispatch_id } => {
                Self::AcknowledgeStart { dispatch_id: *dispatch_id }
            }
            Self::CompleteWork { dispatch_id, result_digest } => {
                Self::CompleteWork { dispatch_id: *dispatch_id, result_digest: *result_digest }
            }
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
            }
            Self::ExhaustWork { work_id, cause_digest } => {
                Self::ExhaustWork { work_id: *work_id, cause_digest: *cause_digest }
            }
            Self::AbandonDispatch { dispatch_id, cause_digest } => {
                Self::AbandonDispatch { dispatch_id: *dispatch_id, cause_digest: *cause_digest }
            }
            Self::PauseScheduler => Self::PauseScheduler,
            Self::ResumeScheduler => Self::ResumeScheduler,
            Self::DrainScheduler => Self::DrainScheduler,
            Self::FinalizeScheduler => Self::FinalizeScheduler,
        }
    }
}
