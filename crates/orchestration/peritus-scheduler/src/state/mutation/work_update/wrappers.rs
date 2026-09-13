//! Public phase-update operations over the verified work mutation kernel.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};

use super::{WorkUpdate, update_work};
#[cfg(verus_only)]
use super::{
    exact_work_update_matches, work_phase_update_matches, work_terminal_update_matches,
    work_update_matches, work_update_preserves_other_state,
};

verus! {

pub fn set_work_phase(
    state: &mut SchedulerState,
    id: WorkId,
    phase: WorkPhase,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        work_update_preserves_other_state(old(state), final(state)),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_sequence() == old(state).spec_sequence(),
        final(state).spec_last_event_id() == old(state).spec_last_event_id(),
        final(state).spec_state_digest() == old(state).spec_state_digest(),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_phase_update_admissible(
                    old(state).spec_work(),
                    old(state).spec_reservations(),
                    id,
                    phase,
                )
            ==> found && final(state).spec_reservation_reducer_ready(),
        work_update_matches(
            old(state).spec_work(), final(state).spec_work(), id, phase, found,
        ),
        work_phase_update_matches(
            old(state).spec_work(), final(state).spec_work(), id, phase, found,
        ),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        final(state).spec_enqueue_ordinal() == old(state).spec_enqueue_ordinal(),
        final(state).spec_dispatch_ordinal() == old(state).spec_dispatch_ordinal(),
        final(state).spec_used_commands() == old(state).spec_used_commands(),
        final(state).spec_terminal() == old(state).spec_terminal(),
{
    let ghost before = state.spec_work();
    let found = update_work(state, id, WorkUpdate::Phase(phase));
    proof {
        reveal(exact_work_update_matches);
        reveal(WorkUpdate::record_matches);
        reveal(work_phase_update_matches);
        reveal(work_update_preserves_other_state);
        assert(work_phase_update_matches(before, state.spec_work(), id, phase, found));
    }
    found
}

pub fn queue_work_retry(state: &mut SchedulerState, id: WorkId) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_sequence() == old(state).spec_sequence(),
        final(state).spec_last_event_id() == old(state).spec_last_event_id(),
        final(state).spec_state_digest() == old(state).spec_state_digest(),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_phase_update_admissible(
                    old(state).spec_work(),
                    old(state).spec_reservations(),
                    id,
                    WorkPhase::Queued,
                )
            ==> found && final(state).spec_reservation_reducer_ready(),
        work_update_matches(
            old(state).spec_work(), final(state).spec_work(), id, WorkPhase::Queued, found,
        ),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        final(state).spec_enqueue_ordinal() == old(state).spec_enqueue_ordinal(),
        final(state).spec_dispatch_ordinal() == old(state).spec_dispatch_ordinal(),
        final(state).spec_used_commands() == old(state).spec_used_commands(),
        final(state).spec_terminal() == old(state).spec_terminal(),
{
    update_work(state, id, WorkUpdate::QueueRetry)
}

pub fn set_work_bypasses(
    state: &mut SchedulerState,
    id: WorkId,
    value: u16,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let mut index = 0;
    while index < state.work.len()
        invariant
            index <= state.work@.len(),
            had_invariant == old(state).spec_reservation_invariant(),
            was_ready == old(state).spec_reservation_reducer_ready(),
            had_invariant ==> state.spec_reservation_invariant(),
            was_ready ==> state.spec_reservation_reducer_ready(),
        decreases state.work@.len() - index,
    {
        if state.work[index].spec().id() == id {
            let ghost before = state.spec_work();
            state.work[index].set_bypasses(value);
            proof {
                assert(0 <= index < before.len());
                assert(before.len() == state.spec_work().len());
                WorkRecord::reservation_lifecycle_fields(
                    &before[index as int],
                    &state.spec_work()[index as int],
                );
                assert forall |other: int| #![auto]
                    0 <= other < before.len() && other != index
                    implies before[other] == state.spec_work()[other] by {
                }
                if had_invariant {
                    crate::verified::actual_reservation_work_update_preserves(
                        state.spec_binding(),
                        state.spec_workers(),
                        before,
                        state.spec_work(),
                        state.spec_reservations(),
                        index as int,
                    );
                }
                if was_ready {
                    crate::verified::lifecycle_work_update_preserves_relations(
                        before,
                        state.spec_work(),
                        state.spec_reservations(),
                        state.spec_used_dispatches(),
                        index as int,
                    );
                    assert(state.spec_reservation_reducer_ready());
                }
            }
            return true;
        }
        index += 1;
    }
    false
}

pub fn terminalize_work(
    state: &mut SchedulerState,
    id: WorkId,
    terminal: WorkTerminal,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        work_update_preserves_other_state(old(state), final(state)),
        final(state).spec_phase() == old(state).spec_phase(),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_phase_update_admissible(
                    old(state).spec_work(),
                    old(state).spec_reservations(),
                    id,
                    WorkPhase::Terminal,
                )
            ==> found && final(state).spec_reservation_reducer_ready(),
        work_update_matches(
            old(state).spec_work(), final(state).spec_work(), id, WorkPhase::Terminal, found,
        ),
        work_terminal_update_matches(
            old(state).spec_work(), final(state).spec_work(), id, terminal, found,
        ),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
{
    let ghost before = state.spec_work();
    let ghost expected_terminal = terminal;
    let found = update_work(state, id, WorkUpdate::Terminal(terminal));
    proof {
        reveal(exact_work_update_matches);
        reveal(WorkUpdate::record_matches);
        reveal(work_terminal_update_matches);
        reveal(work_update_preserves_other_state);
        assert(work_terminal_update_matches(
            before,
            state.spec_work(),
            id,
            expected_terminal,
            found,
        ));
    }
    found
}

} // verus!
