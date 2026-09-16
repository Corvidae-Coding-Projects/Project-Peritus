//! Exact queue accounting for each persisted scheduler semantics.

use crate::{SchedulerSemantics, SchedulerState, WorkPhase, WorkRecord};
use vstd::prelude::*;

#[cfg(test)]
mod tests;

verus! {

/// The categories needed by admission and inert-state validation.
#[derive(Clone, Copy)]
enum Measure {
    Waiting,
    RecoveryPressure,
    LegacyOccupancy,
}

/// A retained item waiting to be dispatched, including dependency and retry waits.
pub open spec fn is_waiting(record: WorkRecord) -> bool {
    matches!(record.spec_phase(),
        WorkPhase::Queued | WorkPhase::WaitingDependencies | WorkPhase::RetryPending)
}

/// Active work that can return to a waiting phase after retryable failure.
pub open spec fn can_return(record: WorkRecord) -> bool {
    matches!(record.spec_phase(), WorkPhase::Reserved | WorkPhase::Running)
        && record.spec_attempts_started()
            < record.spec_definition().spec_maximum_attempts().spec_value()
}

/// Work that still owns a reservation, including cancellation awaiting acknowledgement.
pub open spec fn is_active(record: WorkRecord) -> bool {
    matches!(record.spec_phase(),
        WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling)
}

spec fn selected(record: WorkRecord, measure: Measure) -> bool {
    match measure {
        Measure::Waiting => is_waiting(record),
        Measure::RecoveryPressure => is_waiting(record) || can_return(record),
        Measure::LegacyOccupancy => is_waiting(record) || is_active(record),
    }
}

const fn includes(record: &WorkRecord, measure: Measure) -> (result: bool)
    ensures result == selected(*record, measure),
{
    if matches!(record.phase(),
        WorkPhase::Queued | WorkPhase::WaitingDependencies | WorkPhase::RetryPending)
    {
        true
    } else {
        match measure {
            Measure::Waiting => false,
            Measure::RecoveryPressure =>
                matches!(record.phase(), WorkPhase::Reserved | WorkPhase::Running)
                    && record.attempts_started() < record.spec().maximum_attempts().get(),
            Measure::LegacyOccupancy => matches!(record.phase(),
                WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling),
        }
    }
}

fn count(work: &[WorkRecord], measure: Measure) -> (result: usize)
    ensures
        result as nat == work@.filter(|record: WorkRecord| selected(record, measure)).len(),
        result <= work.len(),
{
    let mut result = 0;
    let mut index = 0;
    proof {
        reveal(Seq::filter);
        assert(work@.take(0) =~= Seq::<WorkRecord>::empty());
    }
    while index < work.len()
        invariant
            index <= work.len(),
            result <= index,
            result as nat == work@.take(index as int)
                .filter(|record: WorkRecord| selected(record, measure)).len(),
        decreases work.len() - index,
    {
        let ghost prefix = work@.take(index as int);
        let record = &work[index];
        if includes(record, measure) {
            result += 1;
        }
        proof {
            prefix.lemma_filter_push(*record, |item: WorkRecord| selected(item, measure));
            assert(prefix.push(*record) =~= work@.take(index as int + 1));
        };
        index += 1;
    }
    proof { assert(work@.take(index as int) =~= work@); }
    result
}

/// Exact admission pressure selected by the immutable aggregate version.
pub open spec fn admission_pressure(state: &SchedulerState) -> nat {
    match state.spec_binding().spec_semantics() {
        SchedulerSemantics::LegacyQueueV1 =>
            state.spec_work().filter(|record: WorkRecord| is_waiting(record)).len(),
        SchedulerSemantics::StrictRecoveryQueueV2 => state.spec_work()
            .filter(|record: WorkRecord| is_waiting(record) || can_return(record)).len(),
    }
}

/// Counts precisely the work reserving a slot for the next admission decision.
pub fn admission_count(state: &SchedulerState) -> (result: usize)
    ensures result as nat == admission_pressure(state), result <= state.spec_work().len(),
{
    match state.binding().semantics() {
        SchedulerSemantics::LegacyQueueV1 => {
            let result = count(state.work(), Measure::Waiting);
            proof {
                assert((|record: WorkRecord| selected(record, Measure::Waiting))
                    =~= (|record: WorkRecord| is_waiting(record)));
            }
            result
        }
        SchedulerSemantics::StrictRecoveryQueueV2 => {
            let result = count(state.work(), Measure::RecoveryPressure);
            proof {
                assert((|record: WorkRecord| selected(record, Measure::RecoveryPressure))
                    =~= (|record: WorkRecord| is_waiting(record) || can_return(record)));
            }
            result
        }
    }
}

/// The finite queue invariant of a decoded or produced checkpoint.
pub open spec fn queue_bound(state: &SchedulerState) -> bool {
    let limits = state.spec_binding().spec_limits();
    match state.spec_binding().spec_semantics() {
        SchedulerSemantics::LegacyQueueV1 => state.spec_work()
            .filter(|record: WorkRecord| is_waiting(record) || is_active(record)).len()
                <= limits.spec_queued_work() as nat + limits.spec_active_reservations() as nat,
        SchedulerSemantics::StrictRecoveryQueueV2 =>
            admission_pressure(state) <= limits.spec_queued_work() as nat,
    }
}

/// Checks the selected finite bound without trusting decoded constructor invariants.
pub fn within_bounds(state: &SchedulerState) -> (result: bool)
    ensures result == queue_bound(state),
{
    let limits = state.binding().limits();
    match state.binding().semantics() {
        SchedulerSemantics::LegacyQueueV1 => {
            let occupancy = count(state.work(), Measure::LegacyOccupancy);
            proof {
                assert((|record: WorkRecord| selected(record, Measure::LegacyOccupancy))
                    =~= (|record: WorkRecord| is_waiting(record) || is_active(record)));
            }
            // Both bounds fit independently; widening before addition also covers
            // inert values before their constructor invariants have been checked.
            occupancy as u128
                <= u128::from(limits.queued_work()) + u128::from(limits.active_reservations())
        }
        SchedulerSemantics::StrictRecoveryQueueV2 =>
            admission_count(state) as u128 <= u128::from(limits.queued_work()),
    }
}

} // verus!
