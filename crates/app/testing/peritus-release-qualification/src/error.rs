//! Stable H4 qualification errors.

use thiserror::Error;

/// Stable category for H4 qualification failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QualificationErrorCode {
    /// A domain value violated its grammar.
    InvalidValue,
    /// A bounded collection was exhausted.
    BoundExceeded,
    /// A canonical identity was duplicated.
    Duplicate,
    /// Evidence bound a different release candidate or campaign.
    BindingMismatch,
    /// Required evidence was absent.
    MissingEvidence,
    /// A signature or content identity could not be verified.
    Integrity,
    /// A fresh-subject adapter violated the collection protocol.
    SubjectProtocol,
    /// Independent audit requirements were not satisfied.
    Audit,
    /// Deterministic serialization failed.
    Serialization,
}

/// Typed H4 qualification failure.
#[derive(Debug, Error)]
#[error("{operation}: {detail}")]
pub struct QualificationError {
    code: QualificationErrorCode,
    operation: &'static str,
    detail: String,
    source: Option<serde_json::Error>,
}

impl QualificationError {
    pub(crate) fn new(
        code: QualificationErrorCode,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { code, operation, detail: detail.into(), source: None }
    }

    pub(crate) fn serialization(operation: &'static str, source: serde_json::Error) -> Self {
        Self {
            code: QualificationErrorCode::Serialization,
            operation,
            detail: "canonical JSON serialization failed".to_owned(),
            source: Some(source),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> QualificationErrorCode {
        self.code
    }

    /// Returns the stable operation label.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
