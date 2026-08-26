//! Accepted production-pointer events and successor checkpoints.

use crate::{PointerCommandKind, ProductionHarnessState};
use peritus_types::{CommandId, EventId, ProjectId, Sha256Digest};

/// Closed pointer event semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerEventKind {
    /// One command was accepted with complete restart-consumable semantics.
    Accepted(PointerCommandKind),
}

/// One exact accepted pointer event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerEvent {
    id: EventId,
    command_id: CommandId,
    project_id: ProjectId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_generation: u64,
    successor_generation: u64,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: PointerEventKind,
}

impl PointerEvent {
    #[allow(
        clippy::too_many_arguments,
        reason = "all pointer event integrity fields remain explicit"
    )]
    pub(crate) const fn from_replay_parts(
        id: EventId,
        command_id: CommandId,
        project_id: ProjectId,
        sequence: u64,
        previous_event: Option<EventId>,
        prior_generation: u64,
        successor_generation: u64,
        prior_state_digest: Sha256Digest,
        policy_digest: Sha256Digest,
        command_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: PointerEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            project_id,
            sequence,
            previous_event,
            prior_generation,
            successor_generation,
            prior_state_digest,
            policy_digest,
            command_digest,
            successor_state_digest,
            kind,
        }
    }
    /// Event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Producing command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Aggregate/project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Positive event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Exact predecessor event.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Pointer generation before application.
    #[must_use]
    pub const fn prior_generation(&self) -> u64 {
        self.prior_generation
    }
    /// Pointer generation after application.
    #[must_use]
    pub const fn successor_generation(&self) -> u64 {
        self.successor_generation
    }
    /// Complete prior checkpoint digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Frozen protected policy-binding digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Complete producing-command digest.
    #[must_use]
    pub const fn command_digest(&self) -> Sha256Digest {
        self.command_digest
    }
    /// Complete successor checkpoint digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Accepted semantic payload.
    #[must_use]
    pub const fn kind(&self) -> &PointerEventKind {
        &self.kind
    }
}

/// One accepted pointer event paired with its complete successor checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerTransition {
    event: PointerEvent,
    state: ProductionHarnessState,
}

impl PointerTransition {
    pub(crate) const fn new(event: PointerEvent, state: ProductionHarnessState) -> Self {
        Self { event, state }
    }
    /// Accepted event.
    #[must_use]
    pub const fn event(&self) -> &PointerEvent {
        &self.event
    }
    /// Complete successor state.
    #[must_use]
    pub const fn state(&self) -> &ProductionHarnessState {
        &self.state
    }
    /// Consumes the transition.
    #[must_use]
    pub fn into_parts(self) -> (PointerEvent, ProductionHarnessState) {
        (self.event, self.state)
    }
}
