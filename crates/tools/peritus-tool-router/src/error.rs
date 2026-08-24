//! Stable router failure vocabulary.

use core::fmt;

/// Stable router error category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterErrorKind {
    /// Descriptor registry is malformed or disagrees with B1.
    Registry,
    /// Tool is unknown or not exposed.
    Exposure,
    /// Call preparation failed before authority.
    Preparation,
    /// Committed authority is absent, stale, or mismatched.
    Authorization,
    /// Dispatcher identity differs from the registered implementation.
    DispatcherIdentity,
    /// Active/completed/replay capacity is exhausted.
    Capacity,
    /// Action identity was reused with different bound bytes.
    ReplayConflict,
    /// A prior non-idempotent outcome must not be repeated.
    PriorOutcome,
    /// Active invocation or control is unknown/unsupported.
    Control,
    /// Dispatcher result/progress is malformed.
    InvalidObservation,
    /// Recovery cannot establish a safe outcome.
    Indeterminate,
}

/// Bounded typed router failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterError {
    kind: RouterErrorKind,
    operation: &'static str,
    detail: &'static str,
}

impl RouterError {
    pub(crate) const fn new(
        kind: RouterErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, detail }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> RouterErrorKind {
        self.kind
    }
    /// Returns the failed router operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Returns bounded stable detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for RouterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl std::error::Error for RouterError {}

impl From<peritus_tool_protocol::ProtocolError> for RouterError {
    fn from(_: peritus_tool_protocol::ProtocolError) -> Self {
        Self::new(
            RouterErrorKind::Preparation,
            "prepare tool call",
            "tool call failed bounded protocol or schema validation",
        )
    }
}
