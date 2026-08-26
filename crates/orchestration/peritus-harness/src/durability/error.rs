//! Typed C0 commit and recovery failures.

use core::fmt;

/// Stable durability failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DurabilityErrorKind {
    /// Command, event, checkpoint, or aggregate bindings disagreed.
    Binding,
    /// Canonical family-79, family-80, or family-81 bytes were rejected.
    Codec,
    /// C0 rejected or could not observe the requested transaction.
    Journal,
    /// Exact command identity was already bound to different canonical bytes.
    IdempotencyConflict,
    /// Immutable events and the complete current checkpoint disagreed.
    Recovery,
    /// An event or complete checkpoint exceeded its configured E1 limit.
    LimitExceeded,
}

/// Required response to a durability failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DurabilityRecovery {
    /// Correct an invalid proposed transition.
    CorrectInput,
    /// Reload immutable events and the authoritative checkpoint.
    ReplayAggregate,
    /// Compare exact C0 and C1 observations before another effect.
    Reconcile,
    /// Isolate integrity-conflicting durable data.
    Quarantine,
}

/// Comparable E1 C0 boundary error with bounded diagnostic context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurabilityError {
    kind: DurabilityErrorKind,
    recovery: DurabilityRecovery,
    detail: String,
}

impl DurabilityError {
    pub(crate) fn new(
        kind: DurabilityErrorKind,
        recovery: DurabilityRecovery,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, recovery, detail: detail.into() }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> DurabilityErrorKind {
        self.kind
    }
    /// Returns the required recovery response.
    #[must_use]
    pub const fn recovery(&self) -> DurabilityRecovery {
        self.recovery
    }
    /// Returns bounded diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for DurabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "harness durability failed ({:?}): {}", self.kind, self.detail)
    }
}

impl std::error::Error for DurabilityError {}
