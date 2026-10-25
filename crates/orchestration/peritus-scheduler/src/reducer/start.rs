//! Exact genesis preparation and event commitment around ordinary hashing.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{
    SchedulerBinding, SchedulerCommand, SchedulerEvent, SchedulerEventKind, SchedulerState,
};

pub(super) fn prepare_genesis(
    binding: &SchedulerBinding,
    event_id: EventId,
    command_id: CommandId,
) -> SchedulerState {
    let owned_binding = binding.clone();

    SchedulerState::genesis(owned_binding, event_id, command_id)
}
pub(super) fn commit_genesis(
    command: &SchedulerCommand,
    binding: &SchedulerBinding,
    state: &mut SchedulerState,
    digest: Sha256Digest,
) -> SchedulerEvent {
    crate::state::mutation::set_state_digest(state, digest);
    let emitted_binding = binding.clone();

    SchedulerEvent::from_wire(
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
    )
}
