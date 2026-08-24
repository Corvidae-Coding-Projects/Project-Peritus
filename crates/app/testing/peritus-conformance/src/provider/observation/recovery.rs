//! Failure, retry, ambiguity, and cancellation observations.

use super::super::{ProviderFailureKind, ProviderTerminal};

/// Direct failed-request observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderFailureObservation {
    kind: ProviderFailureKind,
    terminal: ProviderTerminal,
    transport_requests: u64,
    partial_events: u64,
}

impl ProviderFailureObservation {
    /// Creates one failed-request observation.
    #[must_use]
    pub const fn new(
        kind: ProviderFailureKind,
        terminal: ProviderTerminal,
        transport_requests: u64,
        partial_events: u64,
    ) -> Self {
        Self { kind, terminal, transport_requests, partial_events }
    }
    /// Returns failure class.
    #[must_use]
    pub const fn kind(self) -> ProviderFailureKind {
        self.kind
    }
    /// Returns terminal class.
    #[must_use]
    pub const fn terminal(self) -> ProviderTerminal {
        self.terminal
    }
    /// Returns exact transport request count.
    #[must_use]
    pub const fn transport_requests(self) -> u64 {
        self.transport_requests
    }
    /// Returns normalized events observed before failure.
    #[must_use]
    pub const fn partial_events(self) -> u64 {
        self.partial_events
    }
}

/// One retry-attempt result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAttemptOutcome {
    /// Provider rate-limit response.
    RateLimited,
    /// Retryable connection or server failure.
    TransientFailure,
    /// Bytes may have been accepted and safe recreation is unavailable.
    Ambiguous,
    /// A valid response completed.
    Completed,
}

/// Direct facts for one submission attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderAttemptObservation {
    ordinal: u64,
    outcome: ProviderAttemptOutcome,
    request_bytes_sent: bool,
    events_observed: u64,
    delay_before_millis: u64,
}

impl ProviderAttemptObservation {
    /// Creates one attempt observation.
    #[must_use]
    pub const fn new(
        ordinal: u64,
        outcome: ProviderAttemptOutcome,
        request_bytes_sent: bool,
        events_observed: u64,
        delay_before_millis: u64,
    ) -> Self {
        Self { ordinal, outcome, request_bytes_sent, events_observed, delay_before_millis }
    }
    /// Returns one-based attempt ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }
    /// Returns attempt outcome.
    #[must_use]
    pub const fn outcome(self) -> ProviderAttemptOutcome {
        self.outcome
    }
    /// Returns whether request bytes were sent.
    #[must_use]
    pub const fn request_bytes_sent(self) -> bool {
        self.request_bytes_sent
    }
    /// Returns normalized events observed by this attempt.
    #[must_use]
    pub const fn events_observed(self) -> u64 {
        self.events_observed
    }
    /// Returns delay before this attempt.
    #[must_use]
    pub const fn delay_before_millis(self) -> u64 {
        self.delay_before_millis
    }
}

/// Complete retry/submission observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRetryObservation {
    attempts: Vec<ProviderAttemptObservation>,
    terminal: ProviderTerminal,
    ambiguous: bool,
}

impl ProviderRetryObservation {
    /// Creates one retry observation.
    #[must_use]
    pub const fn new(
        attempts: Vec<ProviderAttemptObservation>,
        terminal: ProviderTerminal,
        ambiguous: bool,
    ) -> Self {
        Self { attempts, terminal, ambiguous }
    }
    /// Returns ordered attempts.
    #[must_use]
    pub fn attempts(&self) -> &[ProviderAttemptObservation] {
        &self.attempts
    }
    /// Returns terminal class.
    #[must_use]
    pub const fn terminal(&self) -> ProviderTerminal {
        self.terminal
    }
    /// Returns whether acceptance remained ambiguous.
    #[must_use]
    pub const fn ambiguous(&self) -> bool {
        self.ambiguous
    }
}

/// Direct cancellation lifecycle observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderCancellationObservation {
    control_observed: bool,
    pending_work_interrupted: bool,
    worker_joined: bool,
    terminal: ProviderTerminal,
    terminal_count: u64,
}

impl ProviderCancellationObservation {
    /// Creates cancellation observations.
    #[must_use]
    pub const fn new(
        control_observed: bool,
        pending_work_interrupted: bool,
        worker_joined: bool,
        terminal: ProviderTerminal,
        terminal_count: u64,
    ) -> Self {
        Self { control_observed, pending_work_interrupted, worker_joined, terminal, terminal_count }
    }
    /// Returns whether cancellation reached the subject.
    #[must_use]
    pub const fn control_observed(self) -> bool {
        self.control_observed
    }
    /// Returns whether pending work was interrupted.
    #[must_use]
    pub const fn pending_work_interrupted(self) -> bool {
        self.pending_work_interrupted
    }
    /// Returns whether the owned worker was joined.
    #[must_use]
    pub const fn worker_joined(self) -> bool {
        self.worker_joined
    }
    /// Returns terminal class.
    #[must_use]
    pub const fn terminal(self) -> ProviderTerminal {
        self.terminal
    }
    /// Returns terminal count.
    #[must_use]
    pub const fn terminal_count(self) -> u64 {
        self.terminal_count
    }
}
