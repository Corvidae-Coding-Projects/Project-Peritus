//! Exact dispatch rejection priority and event construction.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use super::DispatchRejection;
use crate::{DispatchId, SchedulerEventKind, SchedulerPhase, SchedulerReservation, SchedulerState};

verus! {

pub open spec fn dispatch_enabled(state: &SchedulerState) -> bool {
    matches!(state.spec_phase(), SchedulerPhase::Active | SchedulerPhase::Draining)
}

/// State facts required by exact selection and reservation installation.
pub open spec fn dispatch_proof_ready(state: &SchedulerState) -> bool {
    crate::selection::exact_selection_available(state)
        && state.spec_reservation_reducer_ready()
}

pub open spec fn selection_reached(state: &SchedulerState, dispatch_id: DispatchId) -> bool {
    &&& state.spec_reservations().len()
        < state.spec_binding().spec_limits().spec_active_reservations()
    &&& !state.spec_used_dispatches().contains(dispatch_id)
    &&& state.spec_used_dispatches().len() < 65_535
}

pub open spec fn selected_failure(
    state: &SchedulerState,
    rejection: DispatchRejection,
) -> bool {
    exists |work_index: int, worker_index: int|
        #![trigger crate::selection::exact_chosen_pair(state, work_index, worker_index)]
        crate::selection::exact_chosen_pair(state, work_index, worker_index)
            && match rejection {
                DispatchRejection::AttemptOverflow =>
                    state.spec_work()[work_index].spec_attempts_started() == u16::MAX,
                DispatchRejection::AttemptBound => {
                    &&& state.spec_work()[work_index].spec_attempts_started() < u16::MAX
                    &&& state.spec_work()[work_index].spec_attempts_started() + 1
                        > state.spec_work()[work_index]
                            .spec_definition().spec_maximum_attempts().spec_value()
                },
                DispatchRejection::OrdinalOverflow => {
                    &&& state.spec_work()[work_index].spec_attempts_started() < u16::MAX
                    &&& state.spec_work()[work_index].spec_attempts_started() + 1
                        <= state.spec_work()[work_index]
                            .spec_definition().spec_maximum_attempts().spec_value()
                    &&& state.spec_dispatch_ordinal() == u64::MAX
                },
                _ => false,
            }
}

pub open spec fn reservation_identifies_selection(
    state: &SchedulerState,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
    reservation: &SchedulerReservation,
) -> bool {
    &&& reservation.spec_dispatch_id() == dispatch_id
    &&& reservation.spec_dispatch_token() == dispatch_token
    &&& exists |work_index: int, worker_index: int|
        #![trigger crate::selection::exact_chosen_pair(state, work_index, worker_index)]
        crate::selection::exact_chosen_pair(state, work_index, worker_index)
            && reservation.spec_work_id()
                == state.spec_work()[work_index].spec_definition().spec_id()
            && reservation.spec_worker_id()
                == state.spec_workers()[worker_index].spec_descriptor().spec_id()
}

pub open spec fn reserve_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
    result: &Result<SchedulerReservation, DispatchRejection>,
) -> bool {
    match result {
        Err(DispatchRejection::Paused) => false,
        Err(DispatchRejection::ActiveLimit) => {
            &&& *after == *before
            &&& before.spec_reservations().len()
                >= before.spec_binding().spec_limits().spec_active_reservations()
        },
        Err(DispatchRejection::DuplicateDispatch) => {
            &&& *after == *before
            &&& before.spec_reservations().len()
                < before.spec_binding().spec_limits().spec_active_reservations()
            &&& before.spec_used_dispatches().contains(dispatch_id)
        },
        Err(DispatchRejection::HistoryLimit) => {
            &&& *after == *before
            &&& before.spec_reservations().len()
                < before.spec_binding().spec_limits().spec_active_reservations()
            &&& !before.spec_used_dispatches().contains(dispatch_id)
            &&& before.spec_used_dispatches().len() >= 65_535
        },
        Err(DispatchRejection::NoFeasibleWork) => {
            &&& *after == *before
            &&& selection_reached(before, dispatch_id)
            &&& crate::selection::no_dispatch_candidate(before)
        },
        Err(rejection @ (DispatchRejection::AttemptOverflow
            | DispatchRejection::AttemptBound
            | DispatchRejection::OrdinalOverflow)) => {
            &&& *after == *before
            &&& selection_reached(before, dispatch_id)
            &&& selected_failure(before, *rejection)
        },
        Err(DispatchRejection::AdmissionDisappeared) => false,
        Ok(reservation) => {
            &&& selection_reached(before, dispatch_id)
            &&& reservation_identifies_selection(
                before, dispatch_id, dispatch_token, reservation,
            )
            &&& before.spec_reservation_reducer_ready()
                ==> after.spec_reservation_reducer_ready()
        },
    }
}

pub open spec fn command_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
    result: &Result<SchedulerEventKind, DispatchRejection>,
) -> bool {
    match result {
        Err(DispatchRejection::Paused) => {
            &&& *after == *before
            &&& !dispatch_enabled(before)
        },
        Err(rejection) => {
            &&& dispatch_enabled(before)
            &&& (dispatch_proof_ready(before) ==> reserve_matches(
                before,
                after,
                dispatch_id,
                dispatch_token,
                &Err(*rejection),
            ))
        },
        Ok(SchedulerEventKind::WorkReserved { reservation }) => {
            &&& dispatch_enabled(before)
            &&& (dispatch_proof_ready(before) ==> {
                &&& selection_reached(before, dispatch_id)
                &&& reservation_identifies_selection(
                    before, dispatch_id, dispatch_token, reservation,
                )
                &&& before.spec_reservation_reducer_ready()
                    ==> after.spec_reservation_reducer_ready()
            })
        },
        Ok(_) => false,
    }
}

} // verus!
