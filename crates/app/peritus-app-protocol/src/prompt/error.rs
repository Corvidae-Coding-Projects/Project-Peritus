//! Prompt-admission rejection vocabulary.

use core::fmt;

/// Stable category for a rejected prompt operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PromptErrorKind {
    /// A caller-supplied bound is zero.
    InvalidLimit,
    /// A choice, constraint, answer, or cancellation value is malformed.
    InvalidInput,
    /// The answer/cancellation does not echo the exact prompt correlation.
    BindingMismatch,
    /// The caller-supplied live revision differs from the bound revision.
    StaleRevision,
    /// The answer payload is not valid for this prompt kind.
    WrongAnswerKind,
    /// A selected option is not in the bound canonical choice set.
    UnknownChoice,
    /// The prompt already has a terminal answer or cancellation.
    AlreadyTerminal,
}

/// Typed prompt failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptError {
    kind: PromptErrorKind,
    detail: &'static str,
}

impl PromptError {
    pub(crate) const fn new(kind: PromptErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }
    /// Returns the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> PromptErrorKind {
        self.kind
    }
    /// Returns inert diagnostic text.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for PromptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for PromptError {}

pub(super) const fn reject(kind: PromptErrorKind, detail: &'static str) -> PromptError {
    PromptError::new(kind, detail)
}
