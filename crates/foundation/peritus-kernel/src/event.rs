//! Logical event plans emitted by accepted commands.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use peritus_types::{
    ActionId, AttemptId, CommandId, EventId, EventSequence, FindingId, ReviewCycleId,
    RevisionTuple, RunId, SessionId, TurnId,
};
use vstd::prelude::*;

verus! {

/// Stable lifecycle event discriminant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KernelEventKind {
    SessionOpened,
    SessionPaused,
    SessionResumed,
    SessionClosed,
    RunStarted,
    RunPaused,
    RunResumed,
    RunCancelled,
    RunFailed,
    RunExhausted,
    RunRejected,
    AttemptStarted,
    AttemptResumed,
    AttemptSubmitted,
    AttemptFailed,
    AttemptExhausted,
    TurnStarted,
    TurnCompleted,
    TurnFailed,
    TurnCancelled,
    ActionProposed,
    ActionAuthorized,
    ActionDispatched,
    ActionCompleted,
    ActionFailed,
    ActionCancelled,
    ReviewRequested,
    ReviewBegun,
    ReviewSubmitted,
    ReviewInvalidated,
    WaiverRequested,
    WaiverGranted,
    WaiverDenied,
    WaiverInvalidated,
    AcceptanceBegun,
    AcceptanceAccepted,
    AcceptanceNeedsChanges,
}

/// Typed subject of one lifecycle event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KernelSubject {
    Session(SessionId),
    Run(RunId),
    Attempt(AttemptId),
    Turn(TurnId),
    Action(ActionId),
    Review(ReviewCycleId),
    Waiver(FindingId),
    Acceptance(RunId),
}

/// One immutable causal lifecycle event plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelEvent {
    pub(crate) id: EventId,
    pub(crate) command_id: CommandId,
    pub(crate) sequence: EventSequence,
    pub(crate) previous_event_id: Option<EventId>,
    pub(crate) revision: RevisionTuple,
    pub(crate) kind: KernelEventKind,
    pub(crate) subject: KernelSubject,
}

impl KernelEvent {
    pub(crate) const fn new(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event_id: Option<EventId>,
        revision: RevisionTuple,
        kind: KernelEventKind,
        subject: KernelSubject,
    ) -> (result: Self)
        ensures
            result.id == id,
            result.command_id == command_id,
            result.sequence == sequence,
            result.previous_event_id == previous_event_id,
            result.revision == revision,
            result.kind == kind,
            result.subject == subject,
    {
        Self { id, command_id, sequence, previous_event_id, revision, kind, subject }
    }
    /// Returns the event identity.
    #[must_use]
    pub const fn id(self) -> EventId { self.id }
    /// Returns the causative command identity.
    #[must_use]
    pub const fn command_id(self) -> CommandId { self.command_id }
    /// Returns the one-based aggregate sequence.
    #[must_use]
    pub const fn sequence(self) -> EventSequence { self.sequence }
    /// Returns the exact previous event identity.
    #[must_use]
    pub const fn previous_event_id(self) -> Option<EventId> { self.previous_event_id }
    /// Returns the exact lifecycle revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple { self.revision }
    /// Returns the stable event discriminant.
    #[must_use]
    pub const fn kind(self) -> KernelEventKind { self.kind }
    /// Returns the typed event subject.
    #[must_use]
    pub const fn subject(self) -> KernelSubject { self.subject }
}

} // verus!
