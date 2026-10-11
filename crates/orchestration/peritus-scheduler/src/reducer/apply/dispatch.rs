//! Verified reservation mutation selected by the production dispatch command.

use peritus_types::Sha256Digest;

use crate::{
    DispatchId, SchedulerEventKind, SchedulerPhase, SchedulerReservation, SchedulerState, WorkId,
    WorkPhase,
};

#[doc = " Exact ordered rejection of production dispatch admission after its phase check."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchRejection {
    #[doc = " The scheduler lifecycle currently forbids dispatch."]
    Paused,
    #[doc = " The active-reservation collection is full."]
    ActiveLimit,
    #[doc = " The requested dispatch identity is already retained."]
    DuplicateDispatch,
    #[doc = " The durable dispatch-identity history is full."]
    HistoryLimit,
    #[doc = " No queued work and available worker pair is feasible."]
    NoFeasibleWork,
    #[doc = " The selected work attempt counter cannot increment."]
    AttemptOverflow,
    #[doc = " The selected work has reached its configured attempt bound."]
    AttemptBound,
    #[doc = " The durable dispatch ordinal cannot increment."]
    OrdinalOverflow,
    #[doc = " A selected pair unexpectedly ceased to be admissible."]
    AdmissionDisappeared,
}
fn incremented_bypass(state: &SchedulerState, id: WorkId, limit: u16) -> u16 {
    let mut index = 0;
    while index < state.work().len() {
        let record = &state.work()[index];
        if record.spec().id().same(&id) {
            return record.bypasses().checked_add(1).unwrap_or(limit).min(limit);
        }
        index += 1;
    }
    limit
}
#[doc = " Applies the production dispatch admission order and reservation mutation."]
pub fn reserve_next(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> Result<SchedulerReservation, DispatchRejection> {
    if state.reservations().len() >= state.binding().limits().active_reservations() as usize {
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
    while scan < state.work().len() {
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
    while index < feasible_ids.len() {
        let id = feasible_ids[index];
        let value = if id == work_id { 0 } else { incremented_bypass(state, id, bypass_limit) };
        let _ = crate::state::mutation::set_work_bypasses(state, id, value);
        index += 1;
    }
    Ok(reservation)
}
#[doc = " Applies the scheduler phase gate and constructs the exact dispatch event."]
pub(super) fn apply_command(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> Result<SchedulerEventKind, DispatchRejection> {
    if !matches!(state.phase(), SchedulerPhase::Active | SchedulerPhase::Draining) {
        return Err(DispatchRejection::Paused);
    }
    match reserve_next(state, dispatch_id, dispatch_token) {
        Ok(reservation) => Ok(SchedulerEventKind::WorkReserved { reservation }),
        Err(DispatchRejection::Paused) => Err(DispatchRejection::Paused),
        Err(rejection) => Err(rejection),
    }
}
