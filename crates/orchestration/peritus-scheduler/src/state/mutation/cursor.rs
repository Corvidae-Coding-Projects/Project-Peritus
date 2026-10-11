//! Exact event-cursor and state-digest mutations.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::SchedulerState;

#[doc = " Advances exactly one accepted event cursor and retains its command identity."]
pub fn advance_cursor(
    state: &mut SchedulerState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) {
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.used_commands.push(command_id);
}
#[doc = " Installs exactly the digest of the complete post-cursor state."]
pub const fn set_state_digest(state: &mut SchedulerState, digest: Sha256Digest) {
    state.state_digest = digest;
}
