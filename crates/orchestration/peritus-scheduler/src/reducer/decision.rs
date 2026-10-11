//! Exact cursor and event commitment around ordinary successor hashing.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{SchedulerCommand, SchedulerEvent, SchedulerEventKind, SchedulerState};

pub(super) fn prepare_cursor(
    state: &mut SchedulerState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) {
    crate::state::mutation::advance_cursor(state, sequence, event_id, command_id);
}
pub(super) const fn commit_event(
    prior: &SchedulerState,
    command: &SchedulerCommand,
    state: &mut SchedulerState,
    kind: SchedulerEventKind,
    digest: Sha256Digest,
) -> SchedulerEvent {
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
    event
}
