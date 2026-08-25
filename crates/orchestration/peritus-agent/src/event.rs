//! Durable-event representation emitted by the pure reducer.

use crate::{ActivePhase, AgentBinding, AgentCommandKind, AgentLimits, AgentPhase};
use peritus_types::{CommandId, EventId, EventSequence, RevisionNumber, Sha256Digest};

/// Closed event vocabulary. Payloads are the exact accepted command facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEventKind {
    Started { binding: AgentBinding, limits: AgentLimits },
    CommandAccepted(AgentCommandKind),
}

/// One canonically ordered aggregate event with predecessor and successor fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentEvent {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event_id: Option<EventId>,
    prior_phase: Option<AgentPhase>,
    prior_resumable: Option<ActivePhase>,
    prior_revision: Option<RevisionNumber>,
    successor_revision: RevisionNumber,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: AgentEventKind,
}

impl AgentEvent {
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "event predecessor and successor fences remain explicit"
    )]
    pub(super) const fn new(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event_id: Option<EventId>,
        prior_phase: Option<AgentPhase>,
        prior_resumable: Option<ActivePhase>,
        prior_revision: Option<RevisionNumber>,
        successor_revision: RevisionNumber,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: AgentEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            sequence,
            previous_event_id,
            prior_phase,
            prior_resumable,
            prior_revision,
            successor_revision,
            prior_state_digest,
            successor_state_digest,
            kind,
        }
    }
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    #[must_use]
    pub const fn previous_event_id(&self) -> Option<EventId> {
        self.previous_event_id
    }
    #[must_use]
    pub const fn prior_phase(&self) -> Option<AgentPhase> {
        self.prior_phase
    }
    #[must_use]
    pub const fn prior_resumable(&self) -> Option<ActivePhase> {
        self.prior_resumable
    }
    #[must_use]
    pub const fn prior_revision(&self) -> Option<RevisionNumber> {
        self.prior_revision
    }
    #[must_use]
    pub const fn successor_revision(&self) -> RevisionNumber {
        self.successor_revision
    }
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    #[must_use]
    pub const fn kind(&self) -> &AgentEventKind {
        &self.kind
    }
}
