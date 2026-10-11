//! Immutable scheduler events and successor transitions.

mod kind;
mod loss;

pub use kind::SchedulerEventKind;
pub use loss::LossOutcome;

use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, RunId, Sha256Digest};

use crate::{SchedulerSemantics, SchedulerState};

/// One canonical event carrying all predecessor/successor fences.

#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerEvent {
    semantics: SchedulerSemantics,
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    prior_state_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: SchedulerEventKind,
}

impl SchedulerEvent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        semantics: SchedulerSemantics,
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        prior_state_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: SchedulerEventKind,
    ) -> Self {
        Self {
            semantics,
            id,
            command_id,
            sequence,
            previous_event,
            run_id,
            revision,
            prior_state_digest,
            successor_state_digest,
            kind,
        }
    }
    #[doc = " Returns event identity."]
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    #[doc = " Returns causative command identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns one-based aggregate sequence."]
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    #[doc = " Returns exact causal predecessor."]
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    #[doc = " Returns bound run."]
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[doc = " Returns exact immutable revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns predecessor-state digest."]
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    #[doc = " Returns complete successor-state digest."]
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    #[doc = " Borrows accepted semantic fact."]
    #[must_use]
    pub const fn kind(&self) -> &SchedulerEventKind {
        &self.kind
    }
}

impl SchedulerEvent {
    #[doc = " Returns the immutable queue and recovery semantics."]
    #[must_use]
    pub const fn semantics(&self) -> SchedulerSemantics {
        self.semantics
    }
}
impl Clone for SchedulerEvent {
    fn clone(&self) -> Self {
        Self {
            semantics: self.semantics,
            id: self.id,
            command_id: self.command_id,
            sequence: self.sequence,
            previous_event: self.previous_event,
            run_id: self.run_id,
            revision: self.revision,
            prior_state_digest: self.prior_state_digest,
            successor_state_digest: self.successor_state_digest,
            kind: self.kind.clone(),
        }
    }
}

/// Pure accepted event plus complete successor checkpoint.

#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerTransition {
    event: SchedulerEvent,
    state: SchedulerState,
}

impl SchedulerTransition {
    pub(crate) const fn new(event: SchedulerEvent, state: SchedulerState) -> Self {
        Self { event, state }
    }
    #[doc = " Borrows immutable event."]
    #[must_use]
    pub const fn event(&self) -> &SchedulerEvent {
        &self.event
    }
    #[doc = " Borrows complete successor state."]
    #[must_use]
    pub const fn state(&self) -> &SchedulerState {
        &self.state
    }
    #[doc = " Consumes transition after durable commit."]
    #[must_use]
    pub fn into_state(self) -> SchedulerState {
        self.state
    }
}
impl Clone for SchedulerTransition {
    fn clone(&self) -> Self {
        Self { event: self.event.clone(), state: self.state.clone() }
    }
}
