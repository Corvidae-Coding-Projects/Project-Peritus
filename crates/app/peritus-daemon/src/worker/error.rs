//! Stable redaction-safe worker-supervisor failures.

use core::fmt;

/// Stable category for a rejected supervisor operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerSupervisorErrorKind {
    /// A configured or caller-supplied bound is zero or outside its production ceiling.
    InvalidLimit,
    /// New work was submitted after draining began.
    NotAccepting,
    /// The configured active-task ceiling was reached.
    Capacity,
    /// A dispatch identity is already owned by an active task or pending observation.
    DuplicateDispatch,
    /// The durable scheduler reservation has not recorded its start acknowledgement.
    ReservationNotStarted,
    /// No active task owns the requested dispatch identity.
    UnknownDispatch,
    /// No running Tokio runtime is available to own the task.
    RuntimeUnavailable,
}

/// Typed supervisor rejection without repository, model, terminal, or provider content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkerSupervisorError {
    kind: WorkerSupervisorErrorKind,
    detail: &'static str,
}

impl WorkerSupervisorError {
    /// Creates one bounded rejection.
    #[must_use]
    pub(crate) const fn new(kind: WorkerSupervisorErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }

    /// Returns the stable rejection category.
    #[must_use]
    pub(crate) const fn kind(&self) -> WorkerSupervisorErrorKind {
        self.kind
    }

    /// Returns inert diagnostic text.
    #[must_use]
    pub(crate) const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for WorkerSupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for WorkerSupervisorError {}
