//! Worker registration, availability, draining, and removal.

use crate::state::mutation;
use crate::{
    SchedulerError, SchedulerErrorKind, SchedulerEventKind, SchedulerPhase, SchedulerState,
    WorkerDescriptor, WorkerId, WorkerPhase, WorkerRecord,
};

pub(super) fn register(
    state: &mut SchedulerState,
    descriptor: &WorkerDescriptor,
) -> Result<SchedulerEventKind, SchedulerError> {
    if matches!(state.phase(), SchedulerPhase::Draining | SchedulerPhase::DrainingPaused) {
        return Err(crate::reducer::illegal("draining scheduler rejects worker registration"));
    }
    let limits = state.binding().limits();
    descriptor.capacity().validate(limits.resource_dimensions())?;
    if state.workers().len() >= usize::from(limits.workers()) {
        return Err(reject(SchedulerErrorKind::LimitExceeded, "worker retention limit reached"));
    }
    if state.worker(descriptor.id()).is_some() {
        return Err(reject(
            SchedulerErrorKind::IdentityConflict,
            "worker identity is already retained",
        ));
    }
    if !descriptor.capacity().fits_within(state.binding().capacity()) {
        return Err(reject(
            SchedulerErrorKind::ResourceConflict,
            "worker capacity exceeds global scheduler capacity",
        ));
    }
    mutation::insert_worker(state, WorkerRecord::new(descriptor.clone()));
    Ok(SchedulerEventKind::WorkerRegistered { descriptor: descriptor.clone() })
}

pub(super) fn available(
    state: &mut SchedulerState,
    worker_id: WorkerId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let phase = state.worker(worker_id).ok_or_else(|| unknown("worker is not registered"))?.phase();
    if !matches!(phase, WorkerPhase::Draining | WorkerPhase::Lost) {
        return Err(crate::reducer::illegal("worker is not draining or lost"));
    }
    if state.reservations().iter().any(|reservation| reservation.worker_id() == worker_id) {
        return Err(crate::reducer::illegal("worker still owns active dispatches"));
    }
    mutation::worker_mut(state, worker_id)
        .ok_or_else(|| unknown("worker disappeared"))?
        .set_phase(WorkerPhase::Available);
    Ok(SchedulerEventKind::WorkerAvailable { worker_id })
}

pub(super) fn drain(
    state: &mut SchedulerState,
    worker_id: WorkerId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let worker = mutation::worker_mut(state, worker_id)
        .ok_or_else(|| unknown("worker is not registered"))?;
    if !matches!(worker.phase(), WorkerPhase::Available | WorkerPhase::Busy) {
        return Err(crate::reducer::illegal("worker cannot enter draining from its current phase"));
    }
    worker.set_phase(WorkerPhase::Draining);
    Ok(SchedulerEventKind::WorkerDrainRequested { worker_id })
}

pub(super) fn remove(
    state: &mut SchedulerState,
    worker_id: WorkerId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let phase = state.worker(worker_id).ok_or_else(|| unknown("worker is not registered"))?.phase();
    if !matches!(phase, WorkerPhase::Draining | WorkerPhase::Lost)
        || state.reservations().iter().any(|reservation| reservation.worker_id() == worker_id)
    {
        return Err(crate::reducer::illegal(
            "only a quiescent draining or lost worker may be removed",
        ));
    }
    mutation::worker_mut(state, worker_id)
        .ok_or_else(|| unknown("removed worker disappeared"))?
        .set_phase(WorkerPhase::Removed);
    Ok(SchedulerEventKind::WorkerRemoved { worker_id })
}

fn unknown(detail: &'static str) -> SchedulerError {
    reject(SchedulerErrorKind::UnknownIdentity, detail)
}

fn reject(kind: SchedulerErrorKind, detail: &'static str) -> SchedulerError {
    crate::error::reject(kind, detail)
}
