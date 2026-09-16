//! Exact cursor and event commitment around ordinary successor hashing.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use vstd::prelude::*;

use crate::{SchedulerCommand, SchedulerEvent, SchedulerEventKind, SchedulerState};

verus! {

pub(super) open spec fn cursor_preparation_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) -> bool {
    &&& after.spec_sequence() == sequence
    &&& after.spec_last_event_id() == event_id
    &&& after.spec_used_commands() == before.spec_used_commands().push(command_id)
    &&& crate::state::mutation::cursor_preserves_other_state(before, after)
    &&& before.spec_reservation_reducer_ready()
        ==> after.spec_reservation_reducer_ready()
    &&& before.spec_collections_ordered() ==> after.spec_collections_ordered()
    &&& crate::state::queue::queue_bound(before)
        ==> crate::state::queue::queue_bound(after)
}

pub(super) fn prepare_cursor(
    state: &mut SchedulerState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
)
    ensures cursor_preparation_matches(
        old(state), final(state), sequence, event_id, command_id,
    ),
{
    crate::state::mutation::advance_cursor(state, sequence, event_id, command_id);
    proof { reveal(cursor_preparation_matches); };
}

pub(super) open spec fn decision_commit_matches(
    prior: &SchedulerState,
    before_digest: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommand,
    kind: &SchedulerEventKind,
    digest: Sha256Digest,
    event: &SchedulerEvent,
) -> bool {
    &&& crate::state::mutation::digest_preserves_other_state(before_digest, after)
    &&& after.spec_state_digest() == digest
    &&& event.spec_semantics() == command.spec_semantics()
    &&& event.spec_id() == command.spec_event_id()
    &&& event.spec_command_id() == command.spec_command_id()
    &&& event.spec_sequence() == before_digest.spec_sequence()
    &&& event.spec_previous_event() == Some(prior.spec_last_event_id())
    &&& event.spec_run_id() == command.spec_run_id()
    &&& event.spec_revision() == command.spec_revision()
    &&& event.spec_prior_state_digest() == prior.spec_state_digest()
    &&& event.spec_successor_state_digest() == digest
    &&& event.spec_successor_state_digest() == after.spec_state_digest()
    &&& event.spec_kind() == kind
    &&& before_digest.spec_reservation_reducer_ready()
        ==> after.spec_reservation_reducer_ready()
    &&& before_digest.spec_collections_ordered() ==> after.spec_collections_ordered()
    &&& crate::state::queue::queue_bound(before_digest)
        ==> crate::state::queue::queue_bound(after)
}

pub(super) const fn commit_event(
    prior: &SchedulerState,
    command: &SchedulerCommand,
    state: &mut SchedulerState,
    kind: SchedulerEventKind,
    digest: Sha256Digest,
) -> (event: SchedulerEvent)
    ensures decision_commit_matches(
        prior, old(state), final(state), command, &kind, digest, &event,
    ),
{
    crate::state::mutation::set_state_digest(state, digest);
    let event = SchedulerEvent::from_wire(
        command.semantics(),
        command.event_id(),
        command.command_id(),
        state.sequence(),
        Some(prior.last_event_id()),
        command.run_id(),
        command.revision(),
        prior.state_digest(),
        digest,
        kind,
    );
    proof {
        reveal(decision_commit_matches);
        reveal(crate::state::mutation::digest_preserves_other_state);
    }
    event
}

} // verus!
