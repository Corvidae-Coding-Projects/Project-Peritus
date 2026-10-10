//! Reducer-only state mutation and derived invariant maintenance.

mod acknowledge_start;
mod cursor;
mod dependency_action;
mod dependency_budget;

mod dependency_round;
mod entity_insertion;
mod insertion_slots;
mod reservation_command;
mod reservation_remove;
mod reservation_update;
mod reserve;
mod scheduler_phase;
mod set_terminal;
mod work_command;
mod work_update;
mod worker_refresh;
mod worker_update;

pub use acknowledge_start::acknowledge_reservation_start;

pub use cursor::{advance_cursor, set_state_digest};

pub use entity_insertion::{insert_work, insert_worker};
pub use reservation_command::{
    AbandonCommandOutcome, AcknowledgeCancellationOutcome, AcknowledgeStartOutcome,
    CompleteCommandOutcome, FailCommandOutcome, apply_abandon_command,
    apply_acknowledge_cancellation_command, apply_acknowledge_start_command,
    apply_complete_command, apply_fail_command,
};

pub use reservation_remove::remove_reservation;
pub use reservation_update::mark_reservation_started;

pub use reserve::reserve_selected_at;

pub use scheduler_phase::{PhaseCommandOutcome, apply_phase_command};
pub use set_terminal::set_terminal;
pub use work_command::{
    ExhaustCommandOutcome, RetryCommandOutcome, apply_exhaust_command, apply_retry_command,
};
pub use work_update::{
    begin_work_attempt_at, queue_work_retry, release_to_phase, release_to_retry_pending,
    release_to_terminal, set_work_bypasses, set_work_phase, terminalize_work,
};

pub use worker_update::set_worker_phase;

use crate::{DispatchId, SchedulerPhase, SchedulerReservation, SchedulerState};
use insertion_slots::{dispatch_slot, reservation_slot};

use worker_refresh::refresh_worker_phases;

pub fn insert_reservation(state: &mut SchedulerState, value: SchedulerReservation) {
    let at = reservation_slot(&state.reservations, value.dispatch_id());
    state.reservations.insert(at, value);
}

pub fn retain_dispatch_identity(state: &mut SchedulerState, id: DispatchId) {
    let at = dispatch_slot(&state.used_dispatches, id);
    state.used_dispatches.insert(at, id);
}

pub fn next_enqueue_ordinal(state: &mut SchedulerState) -> Option<u64> {
    let value = state.enqueue_ordinal.checked_add(1)?;
    state.enqueue_ordinal = value;
    Some(value)
}
pub const fn increment_dispatch_ordinal(state: &mut SchedulerState) -> bool {
    if state.dispatch_ordinal < u64::MAX {
        state.dispatch_ordinal += 1;
        true
    } else {
        false
    }
}

pub const fn set_phase(state: &mut SchedulerState, phase: SchedulerPhase) {
    state.phase = phase;
}

pub fn refresh(state: &mut SchedulerState) {
    propagate_dependencies(state);
    refresh_worker_phases(state);
}
fn propagate_dependencies(state: &mut SchedulerState) {
    let mut remaining = state.work().len();
    let mut second_half = false;
    while remaining > 0 {
        let changed = dependency_round::apply_dependency_round(state);
        if !changed {
            return;
        }
        dependency_budget::consume(&mut remaining, &mut second_half);
    }
}
