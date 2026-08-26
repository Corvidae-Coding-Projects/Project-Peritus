//! Terminal-state rejection vocabulary.

use core::fmt;

/// Stable category for a rejected terminal operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalErrorKind {
    /// A caller-supplied byte or dimension bound is zero.
    InvalidLimit,
    /// An input, output, or resize value is malformed.
    InvalidInput,
    /// An operation names another attachment/process/request binding.
    BindingMismatch,
    /// Output sequence is not the exact expected sequence.
    UnexpectedSequence,
    /// Output byte offset is not the exact conserved offset.
    UnexpectedOffset,
    /// Output sequence or offset arithmetic overflowed.
    ArithmeticOverflow,
    /// An operation was attempted after a terminal transition.
    AlreadyTerminal,
    /// A repeated detach/cancel fact conflicts with the retained fact.
    TerminalConflict,
}

/// Typed terminal protocol failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalError {
    kind: TerminalErrorKind,
    detail: &'static str,
}

impl TerminalError {
    pub(crate) const fn new(kind: TerminalErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }
    /// Returns the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> TerminalErrorKind {
        self.kind
    }
    /// Returns inert diagnostic text.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for TerminalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for TerminalError {}

pub(super) const fn reject(kind: TerminalErrorKind, detail: &'static str) -> TerminalError {
    TerminalError::new(kind, detail)
}
