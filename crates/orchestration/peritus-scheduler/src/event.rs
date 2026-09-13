//! Immutable scheduler events and successor transitions.

mod kind;
mod loss;

pub use kind::SchedulerEventKind;
pub use loss::LossOutcome;

use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, RunId, Sha256Digest};

use crate::{SchedulerSemantics, SchedulerState};
use vstd::prelude::*;

/// One canonical event carrying all predecessor/successor fences.
#[cfg_attr(verus_keep_ghost, verifier::verify)]
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
    /// Returns event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Returns causative command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns one-based aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns exact causal predecessor.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns exact immutable revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Borrows accepted semantic fact.
    #[must_use]
    pub const fn kind(&self) -> &SchedulerEventKind {
        &self.kind
    }
}

verus! {

impl SchedulerEvent {
    /// Relates every fence, digest, and semantic payload across an event clone.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        &&& left.semantics == right.semantics
        &&& left.id == right.id
        &&& left.command_id == right.command_id
        &&& left.sequence == right.sequence
        &&& left.previous_event == right.previous_event
        &&& left.run_id == right.run_id
        &&& left.revision == right.revision
        &&& left.prior_state_digest == right.prior_state_digest
        &&& left.successor_state_digest == right.successor_state_digest
        &&& SchedulerEventKind::clone_equivalent(&left.kind, &right.kind)
    }

    /// Returns the mathematical immutable scheduler semantics.
    pub closed spec fn spec_semantics(&self) -> SchedulerSemantics { self.semantics }

    /// Returns the immutable queue and recovery semantics.
    #[must_use]
    pub const fn semantics(&self) -> (result: SchedulerSemantics)
        ensures result == self.spec_semantics(),
    {
        self.semantics
    }
}

impl Clone for SchedulerEvent {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
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

} // verus!

/// Pure accepted event plus complete successor checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerTransition {
    event: SchedulerEvent,
    state: SchedulerState,
}

impl SchedulerTransition {
    pub(crate) const fn new(event: SchedulerEvent, state: SchedulerState) -> Self {
        Self { event, state }
    }
    /// Borrows immutable event.
    #[must_use]
    pub const fn event(&self) -> &SchedulerEvent {
        &self.event
    }
    /// Borrows complete successor state.
    #[must_use]
    pub const fn state(&self) -> &SchedulerState {
        &self.state
    }
    /// Consumes transition after durable commit.
    #[must_use]
    pub fn into_state(self) -> SchedulerState {
        self.state
    }
}
