//! Exact queue accounting for each persisted scheduler semantics.

use crate::{SchedulerSemantics, SchedulerState, WorkPhase, WorkRecord};

#[doc = " The categories needed by admission and inert-state validation."]
#[derive(Clone, Copy)]
enum Measure {
    Waiting,
    RecoveryPressure,
    LegacyOccupancy,
}
const fn includes(record: &WorkRecord, measure: Measure) -> bool {
    if matches!(
        record.phase(),
        WorkPhase::Queued | WorkPhase::WaitingDependencies | WorkPhase::RetryPending
    ) {
        true
    } else {
        match measure {
            Measure::Waiting => false,
            Measure::RecoveryPressure => {
                matches!(record.phase(), WorkPhase::Reserved | WorkPhase::Running)
                    && record.attempts_started() < record.spec().maximum_attempts().get()
            }
            Measure::LegacyOccupancy => matches!(
                record.phase(),
                WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
            ),
        }
    }
}
fn count(work: &[WorkRecord], measure: Measure) -> usize {
    let mut result = 0;
    let mut index = 0;
    while index < work.len() {
        let record = &work[index];
        if includes(record, measure) {
            result += 1;
        }
        index += 1;
    }
    result
}
#[doc = " Counts precisely the work reserving a slot for the next admission decision."]
pub fn admission_count(state: &SchedulerState) -> usize {
    match state.binding().semantics() {
        SchedulerSemantics::LegacyQueueV1 => count(state.work(), Measure::Waiting),
        SchedulerSemantics::StrictRecoveryQueueV2 => count(state.work(), Measure::RecoveryPressure),
    }
}
#[doc = " Checks the selected finite bound without trusting decoded constructor invariants."]
pub fn within_bounds(state: &SchedulerState) -> bool {
    let limits = state.binding().limits();
    match state.binding().semantics() {
        SchedulerSemantics::LegacyQueueV1 => {
            let occupancy = count(state.work(), Measure::LegacyOccupancy);
            occupancy as u128
                <= u128::from(limits.queued_work()) + u128::from(limits.active_reservations())
        }
        SchedulerSemantics::StrictRecoveryQueueV2 => {
            admission_count(state) as u128 <= u128::from(limits.queued_work())
        }
    }
}
