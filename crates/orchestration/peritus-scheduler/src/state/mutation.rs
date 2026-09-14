//! Reducer-only state mutation and derived invariant maintenance.

mod acknowledge_start;
mod cursor;
mod dependency_action;
mod dependency_budget;
#[cfg(verus_only)]
mod dependency_measure;
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
#[cfg(verus_only)]
pub(crate) use acknowledge_start::start_target_exists;
pub use cursor::{advance_cursor, set_state_digest};
#[cfg(verus_only)]
pub(crate) use cursor::{cursor_preserves_other_state, digest_preserves_other_state};
pub use entity_insertion::{insert_work, insert_worker};
pub use reservation_command::{
    AbandonCommandOutcome, AcknowledgeCancellationOutcome, AcknowledgeStartOutcome,
    CompleteCommandOutcome, FailCommandOutcome, apply_abandon_command,
    apply_acknowledge_cancellation_command, apply_acknowledge_start_command,
    apply_complete_command, apply_fail_command,
};
#[cfg(verus_only)]
pub(crate) use reservation_command::{
    acknowledge_cancellation_outcome_matches, cancelling_dispatch_cannot_resurrect,
    complete_command_outcome_matches,
};
#[cfg(verus_only)]
pub(crate) use reservation_remove::exact_reservation_removal_matches;
pub use reservation_remove::remove_reservation;
pub use reservation_update::mark_reservation_started;
#[cfg(verus_only)]
pub(crate) use reserve::dispatch_admission_ready;
pub use reserve::reserve_selected_at;
#[cfg(verus_only)]
pub(crate) use scheduler_phase::phase_command_matches;
pub use scheduler_phase::{PhaseCommandOutcome, apply_phase_command};
pub use set_terminal::set_terminal;
pub use work_command::{
    ExhaustCommandOutcome, RetryCommandOutcome, apply_exhaust_command, apply_retry_command,
};
pub use work_update::{
    begin_work_attempt_at, queue_work_retry, release_to_phase, release_to_retry_pending,
    release_to_terminal, set_work_bypasses, set_work_phase, terminalize_work,
};
#[cfg(verus_only)]
pub(crate) use work_update::{
    phase_release_matches, release_target_exists, terminal_release_matches,
    work_phase_update_matches, work_record_update_matches, work_terminal_update_matches,
    work_update_preserves_other_state,
};
pub use worker_update::set_worker_phase;
#[cfg(verus_only)]
pub(crate) use worker_update::{
    worker_phase_state_matches, worker_phase_update_matches, worker_update_preserves_other_state,
};

use crate::{DispatchId, SchedulerPhase, SchedulerReservation, SchedulerState};
use insertion_slots::{dispatch_slot, reservation_slot};
use vstd::prelude::*;
use worker_refresh::refresh_worker_phases;

verus! {
pub fn insert_reservation(state: &mut SchedulerState, value: SchedulerReservation)
    ensures
        exists |at: int| #![auto]
            0 <= at <= old(state).spec_reservations().len()
                && final(state).spec_reservations()
                    == old(state).spec_reservations().insert(at, value),
        final(state).spec_binding().spec_limits()
            == old(state).spec_binding().spec_limits(),
        final(state).spec_binding().spec_capacity().spec_entries()
            == old(state).spec_binding().spec_capacity().spec_entries(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_reservations_ordered()
                && (forall |index: int| 0 <= index < old(state).spec_reservations().len() ==>
                    old(state).spec_reservations()[index].spec_dispatch_id() != value.spec_dispatch_id())
            ==> final(state).spec_reservations_ordered(),
        old(state).spec_reservation_invariant()
                && old(state).spec_reservation_feasible(&value)
            ==> final(state).spec_reservation_invariant(),
{
    let ghost before = state.spec_reservations();
    let at = reservation_slot(&state.reservations, value.dispatch_id());
    proof {
        assert(0 <= (at as int) && (at as int) <= state.spec_reservations().len());
        if state.spec_reservations_ordered()
            && (forall |index: int| 0 <= index < before.len() ==>
                before[index].spec_dispatch_id() != value.spec_dispatch_id())
        {
            SchedulerState::reservation_insertion_ordered(before, value, at as int);
        }
        if state.spec_reservation_invariant() && state.spec_reservation_feasible(&value) {
            crate::verified::actual_reservation_insertion_preserves(
                state.spec_binding(),
                state.spec_workers(),
                state.spec_work(),
                state.spec_reservations(),
                value,
                at as int,
            );
        }
    };
    state.reservations.insert(at, value);
    proof {
        assert(state.spec_reservations() == before.insert(at as int, value));
        assert(exists |insert_at: int| #![auto]
            0 <= insert_at <= before.len()
                && state.spec_reservations() == before.insert(insert_at, value)) by {
        }
    };
}

} // verus!

verus! {


pub fn retain_dispatch_identity(state: &mut SchedulerState, id: DispatchId)
    ensures
        exists |at: int| #![auto]
            0 <= at <= old(state).spec_used_dispatches().len()
                && final(state).spec_used_dispatches()
                    == old(state).spec_used_dispatches().insert(at, id),
        final(state).spec_binding().spec_limits()
            == old(state).spec_binding().spec_limits(),
        final(state).spec_binding().spec_capacity().spec_entries()
            == old(state).spec_binding().spec_capacity().spec_entries(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        old(state).spec_used_dispatches_ordered() && !old(state).spec_used_dispatches().contains(id)
            ==> final(state).spec_used_dispatches_ordered(),
{
    let ghost before = state.spec_used_dispatches();
    let at = dispatch_slot(&state.used_dispatches, id);
    proof {
        if state.spec_used_dispatches_ordered() && !before.contains(id) {
            SchedulerState::dispatch_insertion_ordered(before, id, at as int);
        }
    }
    state.used_dispatches.insert(at, id);
    proof {
        assert(state.spec_used_dispatches() == before.insert(at as int, id));
        assert(exists |insert_at: int| #![auto]
            0 <= insert_at <= before.len()
                && state.spec_used_dispatches() == before.insert(insert_at, id)) by {
        }
    };
}

} // verus!

verus! {

/// Admission ordinals change no other scheduler field.
pub open spec fn enqueue_preserves_other_state(before: &SchedulerState, after: &SchedulerState) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_work() == before.spec_work()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

pub fn next_enqueue_ordinal(state: &mut SchedulerState) -> (result: Option<u64>)
    ensures
        enqueue_preserves_other_state(old(state), final(state)),
        result.is_some() == (old(state).spec_enqueue_ordinal() < u64::MAX),
        match result {
            Some(value) => value == old(state).spec_enqueue_ordinal() + 1
                && final(state).spec_enqueue_ordinal() == value,
            None => *final(state) == *old(state),
        },
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
{
    let value = state.enqueue_ordinal.checked_add(1)?;
    state.enqueue_ordinal = value;
    Some(value)
}

pub const fn increment_dispatch_ordinal(state: &mut SchedulerState) -> (result: bool)
    ensures
        result == (old(state).spec_dispatch_ordinal() < u64::MAX),
        result ==> final(state).spec_dispatch_ordinal()
            == old(state).spec_dispatch_ordinal() + 1,
        !result ==> final(state).spec_dispatch_ordinal()
            == old(state).spec_dispatch_ordinal(),
        final(state).spec_binding().spec_limits()
            == old(state).spec_binding().spec_limits(),
        final(state).spec_binding().spec_capacity().spec_entries()
            == old(state).spec_binding().spec_capacity().spec_entries(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
{
    if state.dispatch_ordinal < u64::MAX {
        state.dispatch_ordinal += 1;
        true
    } else {
        false
    }
}

} // verus!

verus! {

pub const fn set_phase(state: &mut SchedulerState, phase: SchedulerPhase)
    ensures
        final(state).spec_phase() == phase,
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_sequence() == old(state).spec_sequence(),
        final(state).spec_last_event_id() == old(state).spec_last_event_id(),
        final(state).spec_state_digest() == old(state).spec_state_digest(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        final(state).spec_enqueue_ordinal() == old(state).spec_enqueue_ordinal(),
        final(state).spec_dispatch_ordinal() == old(state).spec_dispatch_ordinal(),
        final(state).spec_used_commands() == old(state).spec_used_commands(),
        final(state).spec_terminal() == old(state).spec_terminal(),
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
{
    state.phase = phase;
}

} // verus!

verus! {

pub fn refresh(state: &mut SchedulerState)
    ensures
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
            ==> {
                &&& final(state).spec_reservation_reducer_ready()
                &&& final(state).spec_collections_ordered()
                &&& dependency_round::dependency_fixed_point(final(state))
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    let ghost initial = *state;
    propagate_dependencies(state);
    let ghost dependencies_refreshed = *state;
    refresh_worker_phases(state);
    proof {
        if initial.spec_reservation_reducer_ready()
            && initial.spec_collections_ordered()
        {
            reveal(worker_update_preserves_other_state);
            dependency_round::fixed_point_preserved_by_same_work(
                &dependencies_refreshed,
                state,
            );
        }
    };
}

fn propagate_dependencies(state: &mut SchedulerState)
    ensures
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
            ==> {
                &&& final(state).spec_reservation_reducer_ready()
                &&& final(state).spec_collections_ordered()
                &&& dependency_round::dependency_fixed_point(final(state))
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
        work_update_preserves_other_state(old(state), final(state)),
        dependency_round::work_id_layout_matches(
            old(state).spec_work(), final(state).spec_work(),
        ),
{
    let ghost initial = *state;
    let ghost valid = state.spec_reservation_reducer_ready()
        && state.spec_collections_ordered();
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let mut remaining = state.work().len();
    let mut second_half = false;
    proof {
        reveal(work_update_preserves_other_state);
        reveal(dependency_round::work_id_layout_matches);
        dependency_measure::dependency_measure_is_bounded(initial.spec_work());
        assert(dependency_measure::dependency_measure(state.spec_work())
            < dependency_budget::dependency_budget(remaining as nat, second_half));
    };
    while remaining > 0
        invariant
            valid == (
                old(state).spec_reservation_reducer_ready()
                    && old(state).spec_collections_ordered()
            ),
            had_queue_bound == crate::state::queue::queue_bound(old(state)),
            valid ==> state.spec_reservation_reducer_ready(),
            valid ==> state.spec_collections_ordered(),
            valid && had_queue_bound ==> crate::state::queue::queue_bound(state),
            work_update_preserves_other_state(old(state), state),
            dependency_round::work_id_layout_matches(
                old(state).spec_work(), state.spec_work(),
            ),
            remaining <= initial.spec_work().len(),
            second_half ==> remaining > 0,
            valid ==> dependency_measure::dependency_measure(state.spec_work())
                < dependency_budget::dependency_budget(remaining as nat, second_half),
        decreases dependency_budget::dependency_budget(remaining as nat, second_half),
    {
        let ghost before = *state;
        let changed = dependency_round::apply_dependency_round(state);
        proof {
            reveal(work_update_preserves_other_state);
            reveal(dependency_round::work_id_layout_matches);
            assert forall |index: int| #![trigger old(state).spec_work()[index]]
                0 <= index < old(state).spec_work().len() implies
                    state.spec_work()[index].spec_definition().spec_id()
                        == old(state).spec_work()[index].spec_definition().spec_id() by {
                assert(before.spec_work()[index].spec_definition().spec_id()
                    == old(state).spec_work()[index].spec_definition().spec_id());
            }
        };
        if !changed {
            proof {
                if valid {
                    assert(before.spec_reservation_reducer_ready());
                    assert(before.spec_collections_ordered());
                    assert(state.spec_reservation_reducer_ready());
                    assert(state.spec_collections_ordered());
                    if had_queue_bound {
                        assert(crate::state::queue::queue_bound(&before));
                        assert(crate::state::queue::queue_bound(state));
                    }
                    assert(dependency_round::dependency_fixed_point(state));
                }
                assert(work_update_preserves_other_state(old(state), state));
                assert(dependency_round::work_id_layout_matches(
                    old(state).spec_work(), state.spec_work(),
                ));
            };
            return;
        }
        proof {
            if valid {
                assert(dependency_measure::dependency_measure(state.spec_work())
                    < dependency_measure::dependency_measure(before.spec_work()));
            }
        };
        dependency_budget::consume(&mut remaining, &mut second_half);
        proof {
            if valid {
                assert(dependency_measure::dependency_measure(state.spec_work())
                    < dependency_budget::dependency_budget(remaining as nat, second_half));
            }
        };
    }
    proof {
        assert(remaining == 0);
        if valid {
            dependency_measure::propagation_is_complete(state, remaining as nat, second_half);
        }
    };
}

} // verus!
