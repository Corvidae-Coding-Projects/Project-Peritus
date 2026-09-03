//! Stable redaction-safe product runner failures.

use core::fmt;

/// Stable product-run failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunnerErrorKind {
    /// Caller input or a required initial authority binding was invalid.
    InvalidPrecondition,
    /// Managed repository inspection failed.
    Repository,
    /// A provider request or response failed.
    Provider,
    /// A model response did not satisfy the edit/review contract.
    InvalidModelOutput,
    /// A concrete developer workspace edit or tool operation failed structurally.
    Apply,
    /// Repository gates could not be executed.
    Gate,
    /// A cumulative product-run resource ceiling was exhausted.
    Budget,
    /// The user cancelled the run.
    Cancelled,
    /// A supposedly impossible internal state transition was rejected.
    InternalInvariant,
}

/// Redaction-safe product runner error.
#[derive(Debug)]
pub struct ProductRunnerError {
    kind: ProductRunnerErrorKind,
    operation: &'static str,
    detail: String,
}

impl ProductRunnerError {
    pub(crate) fn new(
        kind: ProductRunnerErrorKind,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, operation, detail: detail.into() }
    }

    /// Stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProductRunnerErrorKind {
        self.kind
    }
    /// Failed operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Redaction-safe diagnostic.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ProductRunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl std::error::Error for ProductRunnerError {}
