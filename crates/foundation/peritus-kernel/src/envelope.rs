//! Exact command/event identity and causal binding.

use peritus_types::{CommandId, EventId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// Identity and expected causal head for one reducer invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommandEnvelope {
    pub(crate) command_id: CommandId,
    pub(crate) event_id: EventId,
    pub(crate) expected_previous_event_id: Option<EventId>,
    pub(crate) revision: RevisionTuple,
}

impl CommandEnvelope {
    /// Creates a complete exact reducer envelope.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        event_id: EventId,
        expected_previous_event_id: Option<EventId>,
        revision: RevisionTuple,
    ) -> Self {
        Self { command_id, event_id, expected_previous_event_id, revision }
    }

    /// Returns the idempotency identity.
    #[must_use]
    pub const fn command_id(self) -> CommandId { self.command_id }
    /// Returns the proposed immutable event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId { self.event_id }
    /// Returns the exact expected aggregate head.
    #[must_use]
    pub const fn expected_previous_event_id(self) -> Option<EventId> {
        self.expected_previous_event_id
    }
    /// Returns the exact authority/evidence revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple { self.revision }
}

} // verus!
