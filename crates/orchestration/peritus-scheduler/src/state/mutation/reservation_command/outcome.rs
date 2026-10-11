//! Deterministic outcomes from production reservation command kernels.

#[doc = " Deterministic result of applying the actual start-acknowledgement payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcknowledgeStartOutcome {
    #[doc = " The reservation became started and its work became running."]
    Applied,
    #[doc = " The actual command was not `AcknowledgeStart`."]
    NotAcknowledgeStartCommand,
    #[doc = " The dispatch identity was not live."]
    DispatchNotActive,
    #[doc = " The dispatch start was already acknowledged."]
    AlreadyAcknowledged,
    #[doc = " The live dispatch disappeared during mutation."]
    DispatchDisappeared,
    #[doc = " The live reservation\'s work disappeared during mutation."]
    WorkDisappeared,
}
#[doc = " Deterministic result of applying the actual successful-completion payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompleteCommandOutcome {
    #[doc = " Running acknowledged work became terminally successful."]
    Applied,
    #[doc = " The actual command was not `CompleteWork`."]
    NotCompleteCommand,
    #[doc = " The dispatch identity was not live."]
    DispatchNotActive,
    #[doc = " The live reservation\'s work was not retained."]
    WorkDisappeared,
    #[doc = " Only acknowledged running work can complete successfully."]
    NotAcknowledgedRunning,
}
#[doc = " Deterministic result of applying the actual failure-classification payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailCommandOutcome {
    #[doc = " Reserved or running work was released under the requested disposition."]
    Applied,
    #[doc = " The actual command was not `FailWork`."]
    NotFailCommand,
    #[doc = " The dispatch identity was not live."]
    DispatchNotActive,
    #[doc = " The live reservation\'s work was not retained."]
    WorkDisappeared,
    #[doc = " Only reserved or running work can report failure."]
    WorkNotFailable,
}
#[doc = " Deterministic result of applying the actual cancellation-acknowledgement payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcknowledgeCancellationOutcome {
    #[doc = " Cancelling ownership was released and the work became cancelled."]
    Applied,
    #[doc = " The actual command was not `AcknowledgeCancellation`."]
    NotAcknowledgeCancellationCommand,
    #[doc = " The dispatch identity was not live."]
    DispatchNotActive,
    #[doc = " The live reservation\'s work was not retained."]
    ReservationWorkDisappeared,
    #[doc = " The live reservation\'s work was not cancelling."]
    WorkNotCancelling,
    #[doc = " The cancelling work disappeared during release."]
    CancellingWorkDisappeared,
}
#[doc = " Deterministic result of applying the actual dispatch-abandonment payload."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbandonCommandOutcome {
    #[doc = " Active ownership was released to abandoned or cancelled terminal state."]
    Applied,
    #[doc = " The actual command was not `AbandonDispatch`."]
    NotAbandonCommand,
    #[doc = " The dispatch identity was not live."]
    DispatchNotActive,
    #[doc = " The live reservation\'s work disappeared during abandonment."]
    WorkDisappeared,
}
