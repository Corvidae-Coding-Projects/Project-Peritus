//! Typed pure aggregate and replay failures.

use core::fmt;

/// Stable aggregate rejection category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AggregateErrorKind {
    /// Command metadata or payload was internally inconsistent.
    InvalidCommand,
    /// Expected sequence, predecessor, or prior-state digest was stale.
    StaleState,
    /// A revision transition violated append-only history.
    Revision,
    /// Pending plan ordering or correlation was impossible.
    Materialization,
    /// A retained history, receipt, state, or diagnostic bound was exceeded.
    LimitExceeded,
    /// Canonical bytes could not be encoded or decoded exactly.
    Codec,
    /// Durable replay or checkpoint equality failed.
    Replay,
    /// One command identity or semantic identity named a different payload.
    Conflict,
}

/// Required recovery response.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AggregateRecovery {
    /// Correct the proposed command.
    CorrectCommand,
    /// Reload current durable aggregate state.
    ReplayAggregate,
    /// Reconcile exact C0 and C1 observations.
    Reconcile,
    /// Quarantine integrity-conflicting state.
    Quarantine,
}

/// Comparable E1 aggregate error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateError {
    kind: AggregateErrorKind,
    recovery: AggregateRecovery,
    detail: String,
}

impl AggregateError {
    pub(crate) fn new(
        kind: AggregateErrorKind,
        recovery: AggregateRecovery,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, recovery, detail: detail.into() }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> AggregateErrorKind {
        self.kind
    }
    /// Returns the required recovery action.
    #[must_use]
    pub const fn recovery(&self) -> AggregateRecovery {
        self.recovery
    }
    /// Returns bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for AggregateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "harness aggregate failed ({:?}): {}", self.kind, self.detail)
    }
}

impl std::error::Error for AggregateError {}
