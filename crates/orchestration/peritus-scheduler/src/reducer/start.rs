//! Exact genesis preparation and event commitment around ordinary hashing.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use vstd::prelude::*;

use crate::{
    SchedulerBinding, SchedulerCommand, SchedulerEvent, SchedulerEventKind, SchedulerState,
};

verus! {

pub(super) open spec fn prepared_genesis_matches(
    binding: &SchedulerBinding,
    event_id: EventId,
    command_id: CommandId,
    state: &SchedulerState,
) -> bool {
    &&& SchedulerBinding::clone_equivalent(binding, state.spec_binding())
    &&& state.spec_phase() == crate::SchedulerPhase::Active
    &&& state.spec_sequence().spec_value() == 1
    &&& state.spec_last_event_id() == event_id
    &&& crate::state::digest_is_zero(state.spec_state_digest())
    &&& state.spec_workers().len() == 0
    &&& state.spec_work().len() == 0
    &&& state.spec_reservations().len() == 0
    &&& state.spec_used_dispatches().len() == 0
    &&& state.spec_enqueue_ordinal() == 0
    &&& state.spec_dispatch_ordinal() == 0
    &&& state.spec_used_commands() == Seq::<CommandId>::empty().push(command_id)
    &&& state.spec_terminal().is_none()
    &&& state.spec_reservation_reducer_ready()
    &&& state.spec_collections_ordered()
}

pub(super) fn prepare_genesis(
    binding: &SchedulerBinding,
    event_id: EventId,
    command_id: CommandId,
) -> (state: SchedulerState)
    ensures prepared_genesis_matches(binding, event_id, command_id, &state),
{
    let owned_binding = binding.clone();
    let state = SchedulerState::genesis(owned_binding, event_id, command_id);
    proof {
        reveal(prepared_genesis_matches);
        SchedulerBinding::clone_preserves_reservation_fields(binding, state.spec_binding());
    }
    state
}

pub(super) open spec fn genesis_commit_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommand,
    binding: &SchedulerBinding,
    digest: Sha256Digest,
    event: &SchedulerEvent,
) -> bool {
    &&& crate::state::mutation::digest_preserves_other_state(before, after)
    &&& after.spec_state_digest() == digest
    &&& event.spec_semantics() == command.spec_semantics()
    &&& event.spec_id() == command.spec_event_id()
    &&& event.spec_command_id() == command.spec_command_id()
    &&& event.spec_sequence().spec_value() == 1
    &&& event.spec_previous_event().is_none()
    &&& event.spec_run_id() == command.spec_run_id()
    &&& event.spec_revision() == command.spec_revision()
    &&& crate::state::digest_is_zero(event.spec_prior_state_digest())
    &&& event.spec_successor_state_digest() == digest
    &&& event.spec_successor_state_digest() == after.spec_state_digest()
    &&& match event.spec_kind() {
        SchedulerEventKind::SchedulerStarted { binding: emitted } =>
            SchedulerBinding::clone_equivalent(binding, emitted),
        _ => false,
    }
    &&& before.spec_reservation_reducer_ready()
        ==> after.spec_reservation_reducer_ready()
    &&& before.spec_collections_ordered() ==> after.spec_collections_ordered()
    &&& crate::state::queue::queue_bound(before)
        ==> crate::state::queue::queue_bound(after)
}

pub(super) fn commit_genesis(
    command: &SchedulerCommand,
    binding: &SchedulerBinding,
    state: &mut SchedulerState,
    digest: Sha256Digest,
) -> (event: SchedulerEvent)
    ensures genesis_commit_matches(old(state), final(state), command, binding, digest, &event),
{
    crate::state::mutation::set_state_digest(state, digest);
    let emitted_binding = binding.clone();
    let event = SchedulerEvent::from_wire(
        command.semantics(),
        command.event_id(),
        command.command_id(),
        EventSequence::first(),
        None,
        command.run_id(),
        command.revision(),
        Sha256Digest::new([0; 32]),
        digest,
        SchedulerEventKind::SchedulerStarted { binding: emitted_binding },
    );
    proof {
        reveal(genesis_commit_matches);
        reveal(crate::state::mutation::digest_preserves_other_state);
        assert forall |index: int| 0 <= index < 32 implies
            event.spec_prior_state_digest().spec_bytes()[index] == 0 by {
        }
    }
    event
}

} // verus!
