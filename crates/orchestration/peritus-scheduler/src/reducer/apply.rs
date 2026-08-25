//! Closed command application against a cloned scheduler state.

mod control;
mod worker_control;

use peritus_codec::sha256;

use crate::state::mutation;
use crate::{
    DispatchId, FailureDisposition, LossOutcome, RecoveryPolicy, SchedulerCommandKind,
    SchedulerError, SchedulerErrorKind, SchedulerEventKind, SchedulerPhase, SchedulerReservation,
    SchedulerState, WorkId, WorkPhase, WorkRecord, WorkSpec, WorkTerminal, WorkerPhase,
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
        SchedulerCommandKind::AcknowledgeStart { dispatch_id } => {
            acknowledge_start(state, *dispatch_id)
        }
        SchedulerCommandKind::CompleteWork { dispatch_id, result_digest } => {
            complete(state, *dispatch_id, *result_digest)
        }
        SchedulerCommandKind::FailWork { dispatch_id, failure_digest, disposition } => {
            fail(state, *dispatch_id, *failure_digest, *disposition)
        }
        SchedulerCommandKind::RetryWork { work_id } => retry(state, *work_id),
        SchedulerCommandKind::CancelWork { work_id } => control::cancel(state, *work_id, false),
        SchedulerCommandKind::CancelWorkTree { work_id } => control::cancel(state, *work_id, true),
        SchedulerCommandKind::AcknowledgeCancellation { dispatch_id } => {
            control::acknowledge_cancel(state, *dispatch_id)
        }
        SchedulerCommandKind::ExhaustWork { work_id, cause_digest } => {
            control::exhaust(state, *work_id, *cause_digest)
        }
        SchedulerCommandKind::AbandonDispatch { dispatch_id, cause_digest } => {
            control::abandon(state, *dispatch_id, *cause_digest)
        }
        SchedulerCommandKind::PauseScheduler => control::pause(state),
        SchedulerCommandKind::ResumeScheduler => control::resume(state),
        SchedulerCommandKind::DrainScheduler => control::drain_scheduler(state),
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
        let reservation = mutation::remove_reservation(state, dispatch_id)
            .ok_or_else(|| unknown("worker-loss reservation disappeared"))?;
        let work_id = reservation.work_id();
        let record = mutation::work_mut(state, work_id)
            .ok_or_else(|| unknown("worker-loss work disappeared"))?;
        let outcome = if record.phase() == WorkPhase::Cancelling {
            record.terminalize(WorkTerminal::Cancelled);
            LossOutcome::Cancelled { dispatch_id, work_id }
        } else {
            match record.spec().recovery() {
                RecoveryPolicy::RetrySafe
                    if record.attempts_started() < record.spec().maximum_attempts().get() =>
                {
                    record.set_phase(WorkPhase::Queued);
                    LossOutcome::Requeued { dispatch_id, work_id }
                }
                RecoveryPolicy::RetrySafe => {
                    record.terminalize(WorkTerminal::Exhausted {
                        cause_digest: sha256(dispatch_id.as_bytes()),
                    });
                    LossOutcome::Exhausted { dispatch_id, work_id }
                }
                RecoveryPolicy::Ambiguous => {
                    record.terminalize(WorkTerminal::Ambiguous { dispatch_id });
                    LossOutcome::Ambiguous { dispatch_id, work_id }
                }
                RecoveryPolicy::Fail => {
                    record.terminalize(WorkTerminal::Failed {
                        failure_digest: sha256(dispatch_id.as_bytes()),
                    });
                    LossOutcome::Failed { dispatch_id, work_id }
                }
            }
        };
        outcomes.push(outcome);
    }
    mutation::worker_mut(state, worker_id)
        .ok_or_else(|| unknown("lost worker disappeared"))?
        .set_phase(WorkerPhase::Lost);
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
    if state.reservations().len() >= usize::from(state.binding().limits().active_reservations()) {
        return Err(limit("active reservation limit reached"));
    }
    if state.used_dispatches().binary_search(&dispatch_id).is_ok() {
        return Err(conflict("dispatch identity is already retained"));
    }
    if state.used_dispatches().len() >= 65_535 {
        return Err(limit("dispatch identity history reached the canonical collection limit"));
    }
    let selection = crate::select_next(state).ok_or_else(|| {
        crate::error::reject(
            SchedulerErrorKind::NoFeasibleWork,
            "no feasible queued work and worker pair exists",
        )
    })?;
    let work_id = selection.work_id();
    let worker_id = selection.worker_id();
    let feasible_ids: Vec<_> = state
        .work()
        .iter()
        .filter(|record| {
            record.phase() == WorkPhase::Queued
                && crate::selection::is_feasible(state, record.spec().id())
        })
        .map(|record| record.spec().id())
        .collect();
    let bypass_limit = state.binding().limits().bypass_count();
    for id in feasible_ids {
        if let Some(record) = mutation::work_mut(state, id) {
            if id == work_id {
                record.set_bypasses(0);
            } else {
                record.set_bypasses(
                    record.bypasses().checked_add(1).unwrap_or(bypass_limit).min(bypass_limit),
                );
            }
        }
    }
    let (owner, revision, resources) = {
        let work = state.work_item(work_id).ok_or_else(|| unknown("selected work disappeared"))?;
        (work.spec().owner(), work.spec().revision(), work.spec().request().clone())
    };
    let attempt = mutation::work_mut(state, work_id)
        .ok_or_else(|| unknown("selected work disappeared"))?
        .begin_attempt()?;
    if !mutation::increment_dispatch_ordinal(state) {
        return Err(limit("dispatch ordinal overflowed"));
    }
    let reservation = SchedulerReservation::new(
        work_id,
        dispatch_id,
        worker_id,
        owner,
        attempt,
        revision,
        resources,
        token,
    );
    mutation::retain_dispatch_identity(state, dispatch_id);
    mutation::insert_reservation(state, reservation.clone());
    Ok(SchedulerEventKind::WorkReserved { reservation })
}

fn acknowledge_start(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let work_id =
        state.reservation(dispatch_id).ok_or_else(|| unknown("dispatch is not active"))?.work_id();
    if state.reservation(dispatch_id).is_some_and(SchedulerReservation::started) {
        return Err(super::illegal("dispatch start is already acknowledged"));
    }
    mutation::reservation_mut(state, dispatch_id)
        .ok_or_else(|| unknown("dispatch disappeared"))?
        .mark_started();
    mutation::work_mut(state, work_id)
        .ok_or_else(|| unknown("reservation work disappeared"))?
        .set_phase(WorkPhase::Running);
    Ok(SchedulerEventKind::WorkStartAcknowledged { dispatch_id })
}

fn complete(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    result: peritus_types::Sha256Digest,
) -> Result<SchedulerEventKind, SchedulerError> {
    let reservation =
        state.reservation(dispatch_id).ok_or_else(|| unknown("dispatch is not active"))?;
    let work = state
        .work_item(reservation.work_id())
        .ok_or_else(|| unknown("reservation work disappeared"))?;
    if !reservation.started() || work.phase() != WorkPhase::Running {
        return Err(super::illegal("only acknowledged running work may succeed"));
    }
    let work_id = reservation.work_id();
    mutation::remove_reservation(state, dispatch_id);
    mutation::work_mut(state, work_id)
        .ok_or_else(|| unknown("reservation work disappeared"))?
        .terminalize(WorkTerminal::Succeeded { result_digest: result });
    Ok(SchedulerEventKind::WorkSucceeded { dispatch_id, result_digest: result })
}

fn fail(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    failure: peritus_types::Sha256Digest,
    disposition: FailureDisposition,
) -> Result<SchedulerEventKind, SchedulerError> {
    let reservation =
        state.reservation(dispatch_id).ok_or_else(|| unknown("dispatch is not active"))?;
    let work_id = reservation.work_id();
    let phase =
        state.work_item(work_id).ok_or_else(|| unknown("reservation work disappeared"))?.phase();
    if !matches!(phase, WorkPhase::Reserved | WorkPhase::Running) {
        return Err(super::illegal("only reserved or running work may fail"));
    }
    mutation::remove_reservation(state, dispatch_id);
    let work = mutation::work_mut(state, work_id)
        .ok_or_else(|| unknown("reservation work disappeared"))?;
    match disposition {
        FailureDisposition::Retryable
            if work.attempts_started() < work.spec().maximum_attempts().get() =>
        {
            work.set_retry_pending(failure);
        }
        FailureDisposition::Retryable => {
            work.terminalize(WorkTerminal::Exhausted { cause_digest: failure });
        }
        FailureDisposition::Failed => {
            work.terminalize(WorkTerminal::Failed { failure_digest: failure });
        }
        FailureDisposition::Ambiguous => work.terminalize(WorkTerminal::Ambiguous { dispatch_id }),
    }
    Ok(SchedulerEventKind::WorkFailed { dispatch_id, failure_digest: failure, disposition })
}

fn retry(
    state: &mut SchedulerState,
    work_id: WorkId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let work = mutation::work_mut(state, work_id).ok_or_else(|| unknown("work is not retained"))?;
    if work.phase() != WorkPhase::RetryPending {
        return Err(super::illegal("work is not retry-pending"));
    }
    if work.attempts_started() >= work.spec().maximum_attempts().get() {
        return Err(super::illegal("work attempt bound is exhausted"));
    }
    work.queue_retry();
    Ok(SchedulerEventKind::WorkRetryQueued { work_id })
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
