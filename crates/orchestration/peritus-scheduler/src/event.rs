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

verus! {

impl SchedulerEvent {
    /// Returns the mathematical event identity.
    pub closed spec fn spec_id(&self) -> EventId { self.id }
    /// Returns the mathematical causative command identity.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }
    /// Returns the mathematical sequence.
    pub closed spec fn spec_sequence(&self) -> EventSequence { self.sequence }
    /// Returns the mathematical causal predecessor.
    pub closed spec fn spec_previous_event(&self) -> Option<EventId> { self.previous_event }
    /// Returns the mathematical run binding.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }
    /// Returns the mathematical revision binding.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }
    /// Returns the mathematical prior state digest.
    pub closed spec fn spec_prior_state_digest(&self) -> Sha256Digest { self.prior_state_digest }
    /// Returns the mathematical successor state digest.
    pub closed spec fn spec_successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Returns the mathematical semantic payload.
    pub closed spec fn spec_kind(&self) -> &SchedulerEventKind { &self.kind }

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
    ) -> (result: Self)
        ensures
            result.spec_semantics() == semantics,
            result.spec_id() == id,
            result.spec_command_id() == command_id,
            result.spec_sequence() == sequence,
            result.spec_previous_event() == previous_event,
            result.spec_run_id() == run_id,
            result.spec_revision() == revision,
            result.spec_prior_state_digest() == prior_state_digest,
            result.spec_successor_state_digest() == successor_state_digest,
            result.spec_kind() == &kind,
    {
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
    pub const fn id(&self) -> (result: EventId)
        ensures result == self.spec_id(),
    {
        self.id
    }
    /// Returns causative command identity.
    #[must_use]
    pub const fn command_id(&self) -> (result: CommandId)
        ensures result == self.spec_command_id(),
    {
        self.command_id
    }
    /// Returns one-based aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> (result: EventSequence)
        ensures result == self.spec_sequence(),
    {
        self.sequence
    }
    /// Returns exact causal predecessor.
    #[must_use]
    pub const fn previous_event(&self) -> (result: Option<EventId>)
        ensures result == self.spec_previous_event(),
    {
        self.previous_event
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> (result: RunId)
        ensures result == self.spec_run_id(),
    {
        self.run_id
    }
    /// Returns exact immutable revision.
    #[must_use]
    pub const fn revision(&self) -> (result: RevisionTuple)
        ensures result == self.spec_revision(),
    {
        self.revision
    }
    /// Returns predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_prior_state_digest(),
    {
        self.prior_state_digest
    }
    /// Returns complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_successor_state_digest(),
    {
        self.successor_state_digest
    }
    /// Borrows accepted semantic fact.
    #[must_use]
    pub const fn kind(&self) -> (result: &SchedulerEventKind)
        ensures result == self.spec_kind(),
    {
        &self.kind
    }
}

} // verus!

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
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerTransition {
    event: SchedulerEvent,
    state: SchedulerState,
}

verus! {

impl SchedulerTransition {
    /// Returns the mathematical accepted event.
    pub closed spec fn spec_event(&self) -> &SchedulerEvent { &self.event }
    /// Returns the mathematical complete successor state.
    pub closed spec fn spec_state(&self) -> &SchedulerState { &self.state }
    /// Relates both authoritative transition values across a clone.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        SchedulerEvent::clone_equivalent(left.spec_event(), right.spec_event())
            && SchedulerState::clone_equivalent(left.spec_state(), right.spec_state())
    }

    pub(crate) const fn new(
        event: SchedulerEvent,
        state: SchedulerState,
    ) -> (result: Self)
        ensures
            result.spec_event() == &event,
            result.spec_state() == &state,
    {
        Self { event, state }
    }
    /// Borrows immutable event.
    #[must_use]
    pub const fn event(&self) -> (result: &SchedulerEvent)
        ensures result == self.spec_event(),
    {
        &self.event
    }
    /// Borrows complete successor state.
    #[must_use]
    pub const fn state(&self) -> (result: &SchedulerState)
        ensures result == self.spec_state(),
    {
        &self.state
    }
    /// Consumes transition after durable commit.
    #[must_use]
    pub fn into_state(self) -> (result: SchedulerState)
        ensures result == *self.spec_state(),
    {
        self.state
    }
}

impl Clone for SchedulerTransition {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self { event: self.event.clone(), state: self.state.clone() }
    }
}

} // verus!
