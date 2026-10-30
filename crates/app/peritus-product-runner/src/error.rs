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
    /// Current user input requests waiting; no task completion is claimed.
    AwaitingUser,
    /// A supposedly impossible internal state transition was rejected.
    InternalInvariant,
}

/// Redaction-safe product runner error.
#[derive(Debug)]
pub struct ProductRunnerError {
    kind: ProductRunnerErrorKind,
    operation: &'static str,
    detail: String,
    wait_revision: Option<u64>,
}

impl ProductRunnerError {
    pub(crate) fn new(
        kind: ProductRunnerErrorKind,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, operation, detail: detail.into(), wait_revision: None }
    }

    pub(crate) const fn awaiting_user(message: String, revision: u64) -> Self {
        Self {
            kind: ProductRunnerErrorKind::AwaitingUser,
            operation: "await user input",
            detail: message,
            wait_revision: Some(revision),
        }
    }

    pub(crate) fn user_wait(&self) -> Option<(&str, u64)> {
        self.wait_revision.map(|revision| (self.detail.as_str(), revision))
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

impl From<peritus_workspace::task_candidate::CandidateError> for ProductRunnerError {
    fn from(error: peritus_workspace::task_candidate::CandidateError) -> Self {
        Self::new(ProductRunnerErrorKind::Repository, error.operation(), error.detail())
    }
}
