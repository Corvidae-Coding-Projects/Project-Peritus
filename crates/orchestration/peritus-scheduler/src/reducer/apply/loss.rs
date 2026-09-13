//! Exact worker-loss selection and classification over pre-transition records.

use crate::{
    DispatchId, LossOutcome, RecoveryPolicy, SchedulerReservation, WorkPhase, WorkRecord, WorkerId,
};
use vstd::prelude::*;

mod batch;
mod release_one;

pub(super) use batch::{WorkerLossError, apply_worker_loss};

verus! {

/// The ordered projection of exactly the reservations belonging to this worker.
pub open spec fn selected_dispatches(
    reservations: Seq<SchedulerReservation>,
    worker: WorkerId,
) -> Seq<DispatchId> {
    reservations.filter(|reservation: SchedulerReservation| reservation.spec_worker_id() == worker)
        .map_values(|reservation: SchedulerReservation| reservation.spec_dispatch_id())
}

/// Snapshots dispatches before any ownership is released.
pub(super) fn dispatches_for_worker(
    reservations: &[SchedulerReservation],
    worker: WorkerId,
) -> (dispatches: Vec<DispatchId>)
    ensures dispatches@ == selected_dispatches(reservations@, worker),
{
    let mut dispatches = Vec::new();
    let mut index = 0;
    proof {
        reveal(Seq::filter);
        assert(reservations@.take(0) =~= Seq::<SchedulerReservation>::empty());
        assert(selected_dispatches(reservations@.take(0), worker) =~= Seq::<DispatchId>::empty());
    }
    while index < reservations.len()
        invariant
            index <= reservations.len(),
            dispatches.len() <= index,
            dispatches@ == selected_dispatches(reservations@.take(index as int), worker),
        decreases reservations.len() - index,
    {
        let ghost prefix = reservations@.take(index as int);
        let reservation = &reservations[index];
        let ghost previous = dispatches@;
        if reservation.worker_id().same(&worker) {
            dispatches.push(reservation.dispatch_id());
        }
        proof {
            prefix.lemma_filter_push(*reservation,
                |item: SchedulerReservation| item.spec_worker_id() == worker);
            assert(prefix.push(*reservation) =~= reservations@.take(index as int + 1));
            let next = selected_dispatches(prefix.push(*reservation), worker);
            if reservation.spec_worker_id() == worker {
                assert(next =~= previous.push(reservation.spec_dispatch_id()));
            } else {
                assert(next =~= previous);
            }
        }
        index += 1;
    }
    proof { assert(reservations@.take(index as int) =~= reservations@); }
    dispatches
}

/// Complete policy relation, including cancellation dominance and exact identities.
pub open spec fn outcome_matches(
    record: WorkRecord,
    dispatch: DispatchId,
    outcome: LossOutcome,
) -> bool {
    let cancelling = record.spec_phase() == WorkPhase::Cancelling;
    let policy = record.spec_definition().spec_recovery();
    let retry_remains = record.spec_attempts_started()
        < record.spec_definition().spec_maximum_attempts().spec_value();
    match outcome {
        LossOutcome::Cancelled { dispatch_id, work_id } => {
            &&& dispatch_id == dispatch
            &&& work_id == record.spec_definition().spec_id()
            &&& cancelling
        }
        LossOutcome::Requeued { dispatch_id, work_id } => {
            &&& dispatch_id == dispatch
            &&& work_id == record.spec_definition().spec_id()
            &&& !cancelling && policy == RecoveryPolicy::RetrySafe && retry_remains
        }
        LossOutcome::Exhausted { dispatch_id, work_id } => {
            &&& dispatch_id == dispatch
            &&& work_id == record.spec_definition().spec_id()
            &&& !cancelling && policy == RecoveryPolicy::RetrySafe && !retry_remains
        }
        LossOutcome::Ambiguous { dispatch_id, work_id } => {
            &&& dispatch_id == dispatch
            &&& work_id == record.spec_definition().spec_id()
            &&& !cancelling && policy == RecoveryPolicy::Ambiguous
        }
        LossOutcome::Failed { dispatch_id, work_id } => {
            &&& dispatch_id == dispatch
            &&& work_id == record.spec_definition().spec_id()
            &&& !cancelling && policy == RecoveryPolicy::Fail
        }
    }
}

/// Classifies the actual retained record before worker-loss release mutates it.
pub(super) const fn classify(record: &WorkRecord, dispatch_id: DispatchId) -> (outcome: LossOutcome)
    ensures outcome_matches(*record, dispatch_id, outcome),
{
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

} // verus!
