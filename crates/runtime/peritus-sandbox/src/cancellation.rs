//! Idempotent, first-reason-wins cancellation.

/// Why a sandbox session was cancelled.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CancellationReason {
    /// User requested cancellation.
    User,
    /// User or calling workflow requested cancellation.
    Requested,
    /// Wall-clock deadline elapsed.
    Deadline,
    /// A resource limit was reached.
    ResourceLimit,
    /// An output bound was reached.
    OutputLimit,
    /// A lease or generation fence invalidated execution.
    LeaseFence,
    /// The owning supervisor is shutting down.
    SupervisorShutdown,
    /// Parent execution was cancelled.
    ParentCancelled,
    /// Backend failed and must tear down.
    BackendFailure,
}

/// Result of an idempotent cancellation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationAcceptance {
    /// This request established the cancellation reason.
    Accepted,
    /// Cancellation was already established; its original reason remains authoritative.
    AlreadyAccepted,
}

/// First-reason-wins cancellation state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CancellationState {
    reason: Option<CancellationReason>,
}

impl CancellationState {
    /// Returns an open state.
    #[must_use]
    pub const fn open() -> Self {
        Self { reason: None }
    }
    /// Returns the accepted reason, if any.
    #[must_use]
    pub const fn reason(self) -> Option<CancellationReason> {
        self.reason
    }
    /// Reports whether cancellation was accepted.
    #[must_use]
    pub const fn is_cancelled(self) -> bool {
        self.reason.is_some()
    }
    /// Accepts the first request and leaves subsequent requests idempotent.
    pub const fn request(&mut self, reason: CancellationReason) -> CancellationAcceptance {
        if self.reason.is_some() {
            CancellationAcceptance::AlreadyAccepted
        } else {
            self.reason = Some(reason);
            CancellationAcceptance::Accepted
        }
    }
}
