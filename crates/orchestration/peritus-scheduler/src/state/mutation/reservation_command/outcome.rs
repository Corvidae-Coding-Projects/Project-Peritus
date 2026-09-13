//! Deterministic outcomes from production reservation command kernels.

use vstd::prelude::*;

verus! {

/// Deterministic result of applying the actual start-acknowledgement payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcknowledgeStartOutcome {
    /// The reservation became started and its work became running.
    Applied,
    /// The actual command was not `AcknowledgeStart`.
    NotAcknowledgeStartCommand,
    /// The dispatch identity was not live.
    DispatchNotActive,
    /// The dispatch start was already acknowledged.
    AlreadyAcknowledged,
    /// The live dispatch disappeared during mutation.
    DispatchDisappeared,
    /// The live reservation's work disappeared during mutation.
    WorkDisappeared,
}

/// Deterministic result of applying the actual successful-completion payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompleteCommandOutcome {
    /// Running acknowledged work became terminally successful.
    Applied,
    /// The actual command was not `CompleteWork`.
    NotCompleteCommand,
    /// The dispatch identity was not live.
    DispatchNotActive,
    /// The live reservation's work was not retained.
    WorkDisappeared,
    /// Only acknowledged running work can complete successfully.
    NotAcknowledgedRunning,
}

/// Deterministic result of applying the actual failure-classification payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailCommandOutcome {
    /// Reserved or running work was released under the requested disposition.
    Applied,
    /// The actual command was not `FailWork`.
    NotFailCommand,
    /// The dispatch identity was not live.
    DispatchNotActive,
    /// The live reservation's work was not retained.
    WorkDisappeared,
    /// Only reserved or running work can report failure.
    WorkNotFailable,
}

/// Deterministic result of applying the actual cancellation-acknowledgement payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcknowledgeCancellationOutcome {
    /// Cancelling ownership was released and the work became cancelled.
    Applied,
    /// The actual command was not `AcknowledgeCancellation`.
    NotAcknowledgeCancellationCommand,
    /// The dispatch identity was not live.
    DispatchNotActive,
    /// The live reservation's work was not retained.
    ReservationWorkDisappeared,
    /// The live reservation's work was not cancelling.
    WorkNotCancelling,
    /// The cancelling work disappeared during release.
    CancellingWorkDisappeared,
}

/// Deterministic result of applying the actual dispatch-abandonment payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbandonCommandOutcome {
    /// Active ownership was released to abandoned or cancelled terminal state.
    Applied,
    /// The actual command was not `AbandonDispatch`.
    NotAbandonCommand,
    /// The dispatch identity was not live.
    DispatchNotActive,
    /// The live reservation's work disappeared during abandonment.
    WorkDisappeared,
}

} // verus!
