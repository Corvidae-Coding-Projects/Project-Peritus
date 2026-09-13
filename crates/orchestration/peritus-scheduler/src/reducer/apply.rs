//! Closed command application against a cloned scheduler state.

mod control;
mod dispatch;
mod worker_control;

use peritus_codec::sha256;

use crate::state::mutation;
use crate::{
    DispatchId, LossOutcome, RecoveryPolicy, SchedulerCommandKind, SchedulerError,
    SchedulerErrorKind, SchedulerEventKind, SchedulerPhase, SchedulerReservation, SchedulerState,
    WorkPhase, WorkRecord, WorkSpec, WorkTerminal, WorkerPhase,
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
    let phase = state.worker(worker_id).ok_or_else(|| unknown("worker is not registered"))?.phase();
    if matches!(phase, WorkerPhase::Lost | WorkerPhase::Removed) {
        return Err(super::illegal("worker is already lost or removed"));
    }
    let dispatches: Vec<_> = state
        .reservations()
        .iter()
        .filter(|reservation| reservation.worker_id() == worker_id)
        .map(SchedulerReservation::dispatch_id)
        .collect();
    let mut outcomes = Vec::with_capacity(dispatches.len());
    for dispatch_id in dispatches {
        let work_id = state
            .reservation(dispatch_id)
            .ok_or_else(|| unknown("worker-loss reservation disappeared"))?
            .work_id();
        let record =
            state.work_item(work_id).ok_or_else(|| unknown("worker-loss work disappeared"))?;
        let (terminal, phase, outcome) = if record.phase() == WorkPhase::Cancelling {
            (Some(WorkTerminal::Cancelled), None, LossOutcome::Cancelled { dispatch_id, work_id })
        } else {
            match record.spec().recovery() {
                RecoveryPolicy::RetrySafe
                    if record.attempts_started() < record.spec().maximum_attempts().get() =>
                {
                    (None, Some(WorkPhase::Queued), LossOutcome::Requeued { dispatch_id, work_id })
                }
                RecoveryPolicy::RetrySafe => (
                    Some(WorkTerminal::Exhausted { cause_digest: sha256(dispatch_id.as_bytes()) }),
                    None,
                    LossOutcome::Exhausted { dispatch_id, work_id },
                ),
                RecoveryPolicy::Ambiguous => (
                    Some(WorkTerminal::Ambiguous { dispatch_id }),
                    None,
                    LossOutcome::Ambiguous { dispatch_id, work_id },
                ),
                RecoveryPolicy::Fail => (
                    Some(WorkTerminal::Failed { failure_digest: sha256(dispatch_id.as_bytes()) }),
                    None,
                    LossOutcome::Failed { dispatch_id, work_id },
                ),
            }
        };
        let released = match (terminal, phase) {
            (Some(terminal), _) => {
                mutation::release_to_terminal(state, dispatch_id, work_id, terminal)
            }
            (None, Some(phase)) => mutation::release_to_phase(state, dispatch_id, work_id, phase),
            (None, None) => None,
        };
        if released.is_none() {
            return Err(unknown("worker-loss work disappeared"));
        }
        outcomes.push(outcome);
    }
    if !mutation::set_worker_phase(state, worker_id, WorkerPhase::Lost) {
        return Err(unknown("lost worker disappeared"));
    }
    Ok(SchedulerEventKind::WorkerLost { worker_id, outcomes })
}

fn admit_work(
    state: &mut SchedulerState,
    spec: &WorkSpec,
) -> Result<SchedulerEventKind, SchedulerError> {
    if matches!(state.phase(), SchedulerPhase::Draining | SchedulerPhase::DrainingPaused) {
        return Err(super::illegal("draining scheduler rejects work admission"));
    }
    let limits = state.binding().limits();
    if state.work().len() >= limits.retained_work() as usize
        || queued_count(state) >= limits.queued_work() as usize
    {
        return Err(limit("work retention or queue limit reached"));
    }
    if state.work_item(spec.id()).is_some() {
        return Err(conflict("work identity is retained"));
    }
    if spec.revision() != state.binding().revision() {
        return Err(binding("work revision differs from scheduler binding"));
    }
    if !spec.request().fits_within(state.binding().capacity()) {
        return Err(resource("work request exceeds global scheduler capacity"));
    }
    for dependency in spec.dependencies() {
        if state.work_item(*dependency).is_none() {
            return Err(unknown("work dependency is absent"));
        }
    }
    if spec.parent().is_some_and(|parent| state.work_item(parent).is_none()) {
        return Err(unknown("work parent is absent"));
    }
    if !state.workers().iter().any(|worker| {
        worker.phase() != WorkerPhase::Removed
            && worker.descriptor().owner() == spec.owner()
            && worker.descriptor().supports(spec.class())
            && spec.request().fits_within(worker.descriptor().capacity())
    }) {
        return Err(crate::error::reject(
            SchedulerErrorKind::InvalidInput,
            "no registered owner worker supports the work execution class and request",
        ));
    }
    let ordinal =
        mutation::next_enqueue_ordinal(state).ok_or_else(|| limit("enqueue ordinal overflowed"))?;
    let phase = if spec.dependencies().is_empty() {
        WorkPhase::Queued
    } else {
        WorkPhase::WaitingDependencies
    };
    mutation::insert_work(state, WorkRecord::new(spec.clone(), phase, ordinal));
    Ok(SchedulerEventKind::WorkAdmitted { spec: spec.clone() })
}

fn dispatch(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    token: peritus_types::Sha256Digest,
) -> Result<SchedulerEventKind, SchedulerError> {
    if !matches!(state.phase(), SchedulerPhase::Active | SchedulerPhase::Draining) {
        return Err(super::illegal("scheduler dispatch is paused"));
    }
    match dispatch::reserve_next(state, dispatch_id, token) {
        Ok(reservation) => Ok(SchedulerEventKind::WorkReserved { reservation }),
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

fn queued_count(state: &SchedulerState) -> usize {
    state
        .work()
        .iter()
        .filter(|record| {
            matches!(
                record.phase(),
                WorkPhase::Queued | WorkPhase::WaitingDependencies | WorkPhase::RetryPending
            )
        })
        .count()
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
