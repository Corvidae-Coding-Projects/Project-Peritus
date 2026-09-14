//! Closed command application against a cloned scheduler state.

mod admission;
mod cancellation;
mod control;
mod dispatch;
mod loss;
mod worker_control;

use peritus_codec::sha256;

use crate::state::mutation;
use crate::{
    DispatchId, SchedulerCommandKind, SchedulerError, SchedulerErrorKind, SchedulerEventKind,
    SchedulerState, WorkSpec,
};

pub(super) fn apply(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    match command {
        SchedulerCommandKind::StartScheduler { .. } => {
            Err(super::illegal("StartScheduler is legal only at genesis"))
        }
        SchedulerCommandKind::RegisterWorker { descriptor } => {
            worker_control::register(state, descriptor)
        }
        SchedulerCommandKind::SetWorkerAvailable { worker_id } => {
            worker_control::available(state, *worker_id)
        }
        SchedulerCommandKind::DrainWorker { worker_id } => worker_control::drain(state, *worker_id),
        SchedulerCommandKind::LoseWorker { worker_id } => lose_worker(state, *worker_id),
        SchedulerCommandKind::RemoveWorker { worker_id } => {
            worker_control::remove(state, *worker_id)
        }
        SchedulerCommandKind::AdmitWork { spec } => admit_work(state, spec),
        SchedulerCommandKind::DispatchNext { dispatch_id, dispatch_token } => {
            dispatch(state, *dispatch_id, *dispatch_token)
        }
        SchedulerCommandKind::AcknowledgeStart { .. } => acknowledge_start(state, command),
        SchedulerCommandKind::CompleteWork { .. } => complete(state, command),
        SchedulerCommandKind::FailWork { .. } => fail(state, command),
        SchedulerCommandKind::RetryWork { .. } => retry(state, command),
        SchedulerCommandKind::CancelWork { work_id } => control::cancel(state, *work_id, false),
        SchedulerCommandKind::CancelWorkTree { work_id } => control::cancel(state, *work_id, true),
        SchedulerCommandKind::AcknowledgeCancellation { .. } => {
            control::acknowledge_cancel(state, command)
        }
        SchedulerCommandKind::ExhaustWork { .. } => control::exhaust(state, command),
        SchedulerCommandKind::AbandonDispatch { .. } => control::abandon(state, command),
        SchedulerCommandKind::PauseScheduler
        | SchedulerCommandKind::ResumeScheduler
        | SchedulerCommandKind::DrainScheduler => control::scheduler_phase(state, command),
        SchedulerCommandKind::FinalizeScheduler => control::finalize(state),
    }
}

fn lose_worker(
    state: &mut SchedulerState,
    worker_id: crate::WorkerId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let plan: Vec<_> = loss::dispatches_for_worker(state.reservations(), worker_id)
        .into_iter()
        .map(|dispatch_id| (dispatch_id, sha256(dispatch_id.as_bytes())))
        .collect();
    loss::apply_worker_loss(state, worker_id, &plan).map_err(|error| match error {
        loss::WorkerLossError::WorkerMissing => unknown("worker is not registered"),
        loss::WorkerLossError::AlreadyLostOrRemoved => {
            super::illegal("worker is already lost or removed")
        }
        loss::WorkerLossError::ReservationDisappeared => {
            unknown("worker-loss reservation disappeared")
        }
        loss::WorkerLossError::WorkDisappeared => unknown("worker-loss work disappeared"),
        loss::WorkerLossError::LostWorkerDisappeared => unknown("lost worker disappeared"),
    })
}

fn admit_work(
    state: &mut SchedulerState,
    spec: &WorkSpec,
) -> Result<SchedulerEventKind, SchedulerError> {
    admission::apply_command(state, spec).map_err(|reason| match reason {
        admission::AdmissionRejection::Draining => {
            super::illegal("draining scheduler rejects work admission")
        }
        admission::AdmissionRejection::CapacityLimit => {
            limit("work retention or queue limit reached")
        }
        admission::AdmissionRejection::DuplicateWork => conflict("work identity is retained"),
        admission::AdmissionRejection::RevisionMismatch => {
            binding("work revision differs from scheduler binding")
        }
        admission::AdmissionRejection::ResourceConflict => {
            resource("work request exceeds global scheduler capacity")
        }
        admission::AdmissionRejection::MissingDependency => unknown("work dependency is absent"),
        admission::AdmissionRejection::MissingParent => unknown("work parent is absent"),
        admission::AdmissionRejection::UnsupportedWork => crate::error::reject(
            SchedulerErrorKind::InvalidInput,
            "no registered owner worker supports the work execution class and request",
        ),
        admission::AdmissionRejection::OrdinalOverflow => limit("enqueue ordinal overflowed"),
    })
}

fn dispatch(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    token: peritus_types::Sha256Digest,
) -> Result<SchedulerEventKind, SchedulerError> {
    match dispatch::apply_command(state, dispatch_id, token) {
        Ok(event) => Ok(event),
        Err(dispatch::DispatchRejection::Paused) => {
            Err(super::illegal("scheduler dispatch is paused"))
        }
        Err(dispatch::DispatchRejection::ActiveLimit) => {
            Err(limit("active reservation limit reached"))
        }
        Err(dispatch::DispatchRejection::DuplicateDispatch) => {
            Err(conflict("dispatch identity is already retained"))
        }
        Err(dispatch::DispatchRejection::HistoryLimit) => {
            Err(limit("dispatch identity history reached the canonical collection limit"))
        }
        Err(dispatch::DispatchRejection::NoFeasibleWork) => Err(crate::error::reject(
            SchedulerErrorKind::NoFeasibleWork,
            "no feasible queued work and worker pair exists",
        )),
        Err(dispatch::DispatchRejection::AttemptOverflow) => {
            Err(limit("work attempt count overflowed"))
        }
        Err(dispatch::DispatchRejection::AttemptBound) => {
            Err(limit("work attempt bound is exhausted"))
        }
        Err(dispatch::DispatchRejection::OrdinalOverflow) => {
            Err(limit("dispatch ordinal overflowed"))
        }
        Err(dispatch::DispatchRejection::AdmissionDisappeared) => {
            Err(unknown("selected dispatch admission disappeared"))
        }
    }
}

fn acknowledge_start(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let SchedulerCommandKind::AcknowledgeStart { dispatch_id } = command else {
        return Err(super::illegal(
            "start acknowledgement dispatcher received a different command",
        ));
    };
    match mutation::apply_acknowledge_start_command(state, command) {
        mutation::AcknowledgeStartOutcome::Applied => {
            Ok(SchedulerEventKind::WorkStartAcknowledged { dispatch_id: *dispatch_id })
        }
        mutation::AcknowledgeStartOutcome::DispatchNotActive => {
            Err(unknown("dispatch is not active"))
        }
        mutation::AcknowledgeStartOutcome::AlreadyAcknowledged => {
            Err(super::illegal("dispatch start is already acknowledged"))
        }
        mutation::AcknowledgeStartOutcome::DispatchDisappeared => {
            Err(unknown("dispatch disappeared"))
        }
        mutation::AcknowledgeStartOutcome::WorkDisappeared => {
            Err(unknown("reservation work disappeared"))
        }
        mutation::AcknowledgeStartOutcome::NotAcknowledgeStartCommand => {
            Err(super::illegal("start acknowledgement dispatcher received a different command"))
        }
    }
}

fn complete(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let SchedulerCommandKind::CompleteWork { dispatch_id, result_digest } = command else {
        return Err(super::illegal("completion dispatcher received a different command"));
    };
    match mutation::apply_complete_command(state, command) {
        mutation::CompleteCommandOutcome::Applied => Ok(SchedulerEventKind::WorkSucceeded {
            dispatch_id: *dispatch_id,
            result_digest: *result_digest,
        }),
        mutation::CompleteCommandOutcome::DispatchNotActive => {
            Err(unknown("dispatch is not active"))
        }
        mutation::CompleteCommandOutcome::WorkDisappeared => {
            Err(unknown("reservation work disappeared"))
        }
        mutation::CompleteCommandOutcome::NotAcknowledgedRunning => {
            Err(super::illegal("only acknowledged running work may succeed"))
        }
        mutation::CompleteCommandOutcome::NotCompleteCommand => {
            Err(super::illegal("completion dispatcher received a different command"))
        }
    }
}

fn fail(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let SchedulerCommandKind::FailWork { dispatch_id, failure_digest, disposition } = command
    else {
        return Err(super::illegal("failure dispatcher received a different command"));
    };
    match mutation::apply_fail_command(state, command) {
        mutation::FailCommandOutcome::Applied => Ok(SchedulerEventKind::WorkFailed {
            dispatch_id: *dispatch_id,
            failure_digest: *failure_digest,
            disposition: *disposition,
        }),
        mutation::FailCommandOutcome::DispatchNotActive => Err(unknown("dispatch is not active")),
        mutation::FailCommandOutcome::WorkDisappeared => {
            Err(unknown("reservation work disappeared"))
        }
        mutation::FailCommandOutcome::WorkNotFailable => {
            Err(super::illegal("only reserved or running work may fail"))
        }
        mutation::FailCommandOutcome::NotFailCommand => {
            Err(super::illegal("failure dispatcher received a different command"))
        }
    }
}

fn retry(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> Result<SchedulerEventKind, SchedulerError> {
    let SchedulerCommandKind::RetryWork { work_id } = command else {
        return Err(super::illegal("retry dispatcher received a non-retry command"));
    };
    match mutation::apply_retry_command(state, command) {
        mutation::RetryCommandOutcome::Applied => {
            Ok(SchedulerEventKind::WorkRetryQueued { work_id: *work_id })
        }
        mutation::RetryCommandOutcome::WorkNotRetained => Err(unknown("work is not retained")),
        mutation::RetryCommandOutcome::WorkNotRetryPending => {
            Err(super::illegal("work is not retry-pending"))
        }
        mutation::RetryCommandOutcome::AttemptBoundExhausted => {
            Err(super::illegal("work attempt bound is exhausted"))
        }
        mutation::RetryCommandOutcome::WorkDisappeared => Err(unknown("work disappeared")),
        mutation::RetryCommandOutcome::NotRetryCommand => {
            Err(super::illegal("retry dispatcher received a non-retry command"))
        }
    }
}

fn limit(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::LimitExceeded, detail)
}
fn conflict(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::IdentityConflict, detail)
}
fn unknown(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::UnknownIdentity, detail)
}
fn binding(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::BindingMismatch, detail)
}
fn resource(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::ResourceConflict, detail)
}
