//! Exact worker-loss selection and classification over pre-transition records.

use crate::{
    DispatchId, LossOutcome, RecoveryPolicy, SchedulerReservation, WorkPhase, WorkRecord, WorkerId,
};

mod batch;
mod release_one;

pub(super) use batch::{WorkerLossError, apply_worker_loss};

#[doc = " Snapshots dispatches before any ownership is released."]
pub(super) fn dispatches_for_worker(
    reservations: &[SchedulerReservation],
    worker: WorkerId,
) -> Vec<DispatchId> {
    let mut dispatches = Vec::new();
    let mut index = 0;
    while index < reservations.len() {
        let reservation = &reservations[index];
        if reservation.worker_id().same(&worker) {
            dispatches.push(reservation.dispatch_id());
        }
        index += 1;
    }
    dispatches
}
#[doc = " Classifies the actual retained record before worker-loss release mutates it."]
pub(super) const fn classify(record: &WorkRecord, dispatch_id: DispatchId) -> LossOutcome {
    let work_id = record.spec().id();
    if record.phase().same(WorkPhase::Cancelling) {
        LossOutcome::Cancelled { dispatch_id, work_id }
    } else {
        match record.spec().recovery() {
            RecoveryPolicy::RetrySafe => {
                if record.attempts_started() < record.spec().maximum_attempts().get() {
                    LossOutcome::Requeued { dispatch_id, work_id }
                } else {
                    LossOutcome::Exhausted { dispatch_id, work_id }
                }
            }
            RecoveryPolicy::Ambiguous => LossOutcome::Ambiguous { dispatch_id, work_id },
            RecoveryPolicy::Fail => LossOutcome::Failed { dispatch_id, work_id },
        }
    }
}
