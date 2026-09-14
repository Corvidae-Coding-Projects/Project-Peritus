//! Exact event-cursor and state-digest mutations.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use vstd::prelude::*;

use crate::SchedulerState;

verus! {

/// Every field outside the event cursor remains unchanged.
pub open spec fn cursor_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_work() == before.spec_work()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_terminal() == before.spec_terminal()
}

/// Advances exactly one accepted event cursor and retains its command identity.
pub fn advance_cursor(
    state: &mut SchedulerState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
)
    ensures
        final(state).spec_sequence() == sequence,
        final(state).spec_last_event_id() == event_id,
        final(state).spec_used_commands()
            == old(state).spec_used_commands().push(command_id),
        cursor_preserves_other_state(old(state), final(state)),
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered()
            ==> final(state).spec_collections_ordered(),
        crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.used_commands.push(command_id);
    proof {
        reveal(cursor_preserves_other_state);
        reveal(SchedulerState::spec_reservation_invariant);
        reveal(SchedulerState::spec_reservation_reducer_ready);
        reveal(SchedulerState::spec_collections_ordered);
        reveal(crate::state::queue::queue_bound);
    };
}

/// Every field outside the canonical state digest remains unchanged.
pub open spec fn digest_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_work() == before.spec_work()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

/// Installs exactly the digest of the complete post-cursor state.
pub const fn set_state_digest(state: &mut SchedulerState, digest: Sha256Digest)
    ensures
        final(state).spec_state_digest() == digest,
        digest_preserves_other_state(old(state), final(state)),
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered()
            ==> final(state).spec_collections_ordered(),
        crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    state.state_digest = digest;
    proof {
        reveal(digest_preserves_other_state);
        reveal(SchedulerState::spec_reservation_invariant);
        reveal(SchedulerState::spec_reservation_reducer_ready);
        reveal(SchedulerState::spec_collections_ordered);
        reveal(crate::state::queue::queue_bound);
    };
}

} // verus!
