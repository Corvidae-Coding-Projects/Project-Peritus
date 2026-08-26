//! Daemon-control rejection vocabulary.

use core::fmt;

/// Stable category for a rejected daemon-control observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DaemonControlErrorKind {
    /// A caller-supplied collection or text bound is zero.
    InvalidLimit,
    /// A status, progress, or completion value is malformed.
    InvalidInput,
    /// A heartbeat sequence or nonce is replayed or noncontiguous.
    HeartbeatOrdering,
    /// A shutdown message names another request/correlation.
    BindingMismatch,
    /// The requested shutdown transition is not legal.
    IllegalTransition,
    /// A terminal shutdown fact conflicts with the retained fact.
    TerminalConflict,
}

/// Typed daemon-control failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonControlError {
    kind: DaemonControlErrorKind,
    detail: &'static str,
}

impl DaemonControlError {
    pub(crate) const fn new(kind: DaemonControlErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }
    /// Returns the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> DaemonControlErrorKind {
        self.kind
    }
    /// Returns inert diagnostic text.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for DaemonControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for DaemonControlError {}

pub(super) const fn reject(
    kind: DaemonControlErrorKind,
    detail: &'static str,
) -> DaemonControlError {
    DaemonControlError::new(kind, detail)
}
