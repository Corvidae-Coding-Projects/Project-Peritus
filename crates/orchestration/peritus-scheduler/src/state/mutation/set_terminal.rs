//! Exact terminal-state mutation.

use crate::{SchedulerPhase, SchedulerState, SchedulerTerminal};
use vstd::prelude::*;

verus! {

/// Commits an already evaluated terminal summary and frames every other state field.
pub fn set_terminal(state: &mut SchedulerState, terminal: SchedulerTerminal)
    ensures
        final(state).spec_phase() == SchedulerPhase::Terminal,
        final(state).spec_terminal() == Some(terminal),
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
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered()
            ==> final(state).spec_collections_ordered(),
{
    state.phase = SchedulerPhase::Terminal;
    state.terminal = Some(terminal);
}

} // verus!
