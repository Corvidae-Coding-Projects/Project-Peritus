//! Verified reservation mutation selected by the production dispatch command.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase};

verus! {

/// Exact ordered rejection of production dispatch admission after its phase check.
pub enum DispatchRejection {
    /// The active-reservation collection is full.
    ActiveLimit,
    /// The requested dispatch identity is already retained.
    DuplicateDispatch,
    /// The durable dispatch-identity history is full.
    HistoryLimit,
    /// No queued work and available worker pair is feasible.
    NoFeasibleWork,
    /// The selected work attempt counter cannot increment.
    AttemptOverflow,
    /// The selected work has reached its configured attempt bound.
    AttemptBound,
    /// The durable dispatch ordinal cannot increment.
    OrdinalOverflow,
    /// A selected pair unexpectedly ceased to be admissible.
    AdmissionDisappeared,
}

fn incremented_bypass(state: &SchedulerState, id: WorkId, limit: u16) -> u16 {
    let mut index = 0;
    while index < state.work().len()
        invariant index <= state.spec_work().len(),
        decreases state.spec_work().len() - index,
    {
        let record = &state.work()[index];
        if record.spec().id().same(&id) {
            return record.bypasses().checked_add(1).unwrap_or(limit).min(limit);
        }
        index += 1;
    }
    limit
}

/// Applies the production dispatch admission order and reservation mutation.
pub fn reserve_next(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> (result: Result<SchedulerReservation, DispatchRejection>)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
{
    let ghost was_ready = state.spec_reservation_reducer_ready();
    if state.reservations().len()
        >= state.binding().limits().active_reservations() as usize
    {
        return Err(DispatchRejection::ActiveLimit);
    }
    if crate::identity::dispatch_id_is_used(state.used_dispatches(), dispatch_id) {
        return Err(DispatchRejection::DuplicateDispatch);
    }
    if state.used_dispatches().len() >= 65_535 {
        return Err(DispatchRejection::HistoryLimit);
    }
    let selected = crate::selection::select_next_for_dispatch(state);
    let Some((work_index, worker_index)) = selected else {
        return Err(DispatchRejection::NoFeasibleWork);
    };
    let work_id = state.work()[work_index].spec().id();
    let mut feasible_ids: Vec<WorkId> = Vec::new();
    let mut scan = 0;
    while scan < state.work().len()
        invariant
            scan <= state.spec_work().len(),
            was_ready == state.spec_reservation_reducer_ready(),
        decreases state.spec_work().len() - scan,
    {
        let record = &state.work()[scan];
        if record.phase().same(WorkPhase::Queued)
            && crate::selection::is_feasible(state, record.spec().id())
        {
            feasible_ids.push(record.spec().id());
        }
        scan += 1;
    }
    let selected_work = &state.work()[work_index];
    if selected_work.attempts_started() == u16::MAX {
        return Err(DispatchRejection::AttemptOverflow);
    }
    if selected_work.attempts_started() + 1 > selected_work.spec().maximum_attempts().get() {
        return Err(DispatchRejection::AttemptBound);
    }
    if state.dispatch_ordinal() == u64::MAX {
        return Err(DispatchRejection::OrdinalOverflow);
    }
    proof {
        if was_ready {
            reveal(crate::state::mutation::dispatch_admission_ready);
            assert(crate::state::mutation::dispatch_admission_ready(
                state, work_index as int, worker_index as int, dispatch_id,
            ));
        }
    }
    let reservation = crate::state::mutation::reserve_selected_at(
        state,
        work_index,
        worker_index,
        dispatch_id,
        dispatch_token,
    );
    let Some(reservation) = reservation else {
        return Err(DispatchRejection::AdmissionDisappeared);
    };
    let bypass_limit = state.binding().limits().bypass_count();
    let mut index = 0;
    while index < feasible_ids.len()
        invariant
            index <= feasible_ids@.len(),
            was_ready ==> state.spec_reservation_reducer_ready(),
        decreases feasible_ids@.len() - index,
    {
        let id = feasible_ids[index];
        let value = if id == work_id {
            0
        } else {
            incremented_bypass(state, id, bypass_limit)
        };
        let _ = crate::state::mutation::set_work_bypasses(state, id, value);
        index += 1;
    }
    Ok(reservation)
}

} // verus!
