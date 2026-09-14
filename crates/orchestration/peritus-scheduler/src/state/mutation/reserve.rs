//! Verified production mutation that installs one selected reservation.

mod prepare;
mod proof;

use crate::{DispatchId, SchedulerReservation, SchedulerState};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Exact pre-state admission needed to install the selector's chosen reservation.
pub open spec fn dispatch_admission_ready(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
) -> bool {
    &&& state.spec_reservation_reducer_ready()
    &&& crate::selection::selected_pair_admitted(state, work_index, worker_index)
    &&& !state.spec_used_dispatches().contains(dispatch_id)
    &&& state.spec_reservations().len()
        < state.spec_binding().spec_limits().spec_active_reservations()
    &&& state.spec_dispatch_ordinal() < u64::MAX
    &&& state.spec_work()[work_index].spec_attempts_started()
        < state.spec_work()[work_index]
            .spec_definition().spec_maximum_attempts().spec_value()
}

/// Begins and durably binds one selector-admitted work/worker pair.
pub fn reserve_selected_at(
    state: &mut SchedulerState,
    work_index: usize,
    worker_index: usize,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> (result: Option<SchedulerReservation>)
    ensures
        dispatch_admission_ready(
            old(state), work_index as int, worker_index as int, dispatch_id,
        ) ==> result.is_some() && final(state).spec_reservation_reducer_ready(),
        match result {
            Some(reservation) => {
                &&& work_index < old(state).spec_work().len()
                &&& worker_index < old(state).spec_workers().len()
                &&& reservation.spec_work_id()
                    == old(state).spec_work()[work_index as int]
                        .spec_definition().spec_id()
                &&& reservation.spec_dispatch_id() == dispatch_id
                &&& reservation.spec_dispatch_token() == dispatch_token
                &&& reservation.spec_worker_id()
                    == old(state).spec_workers()[worker_index as int]
                        .spec_descriptor().spec_id()
            },
            None => true,
        },
{
    let ghost before_binding = state.spec_binding();
    let ghost before_workers = state.spec_workers();
    let ghost before_work = state.spec_work();
    let ghost before_reservations = state.spec_reservations();
    let ghost before_used = state.spec_used_dispatches();
    let ghost admission_ready = dispatch_admission_ready(
        state,
        work_index as int,
        worker_index as int,
        dispatch_id,
    );
    let ghost dispatch_was_not_live = forall |reservation_index: int| #![auto]
        0 <= reservation_index < state.spec_reservations().len() ==>
            state.spec_reservations()[reservation_index].spec_dispatch_id() != dispatch_id;
    let ghost work_was_unreserved = forall |reservation_index: int| #![auto]
        0 <= reservation_index < state.spec_reservations().len() ==>
            state.spec_reservations()[reservation_index].spec_work_id()
                != state.spec_work()[work_index as int].spec_definition().spec_id();
    proof {
        if admission_ready {
            reveal(dispatch_admission_ready);
            proof::admitted_pair_has_no_live_ownership(
                state,
                work_index as int,
                worker_index as int,
                dispatch_id,
            );
            assert(dispatch_was_not_live);
            assert(work_was_unreserved);
        }
    }
    let prepared = prepare::selected_reservation(
        state,
        work_index,
        worker_index,
        dispatch_id,
        dispatch_token,
    );
    proof {
        if admission_ready {
            reveal(dispatch_admission_ready);
            reveal(crate::selection::selected_pair_admitted);
            reveal(crate::selection::selected_pair_feasible);
            assert(prepare::preparation_available(
                old(state), work_index as int, worker_index as int,
            ));
            assert(prepared.is_some());
        }
    }
    let reservation = prepared?;
    let stored = reservation.clone();
    proof {
        if admission_ready {
            reveal(dispatch_admission_ready);
            reveal(crate::selection::selected_pair_admitted);
            reveal(crate::selection::selected_pair_feasible);
            reveal(prepare::preparation_matches);
            proof::admitted_reservation_is_feasible(
                state,
                before_binding,
                before_workers,
                before_work,
                before_reservations,
                work_index as int,
                worker_index as int,
                dispatch_id,
                &reservation,
                &stored,
            );
            SchedulerReservation::clone_fields(&reservation, &stored);
            assert(!stored.spec_started());
        }
    }
    super::retain_dispatch_identity(state, dispatch_id);
    super::insert_reservation(state, stored);
    proof {
        if admission_ready {
            reveal(dispatch_admission_ready);
            reveal(prepare::preparation_matches);
            reveal(SchedulerState::spec_reservation_reducer_ready);
            proof::inserted_state_is_ready(
                state,
                before_work,
                before_reservations,
                before_used,
                work_index as int,
                dispatch_id,
                stored,
            );
            assert(state.spec_reservation_reducer_ready());
        }
        assert(before_binding == old(state).spec_binding());
        assert(before_workers == old(state).spec_workers());
        assert(before_reservations == old(state).spec_reservations());
    }
    Some(reservation)
}

} // verus!
