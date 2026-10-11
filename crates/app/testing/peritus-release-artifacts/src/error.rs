//! Stable errors for artifact and evidence validation.

use thiserror::Error;

/// Stable category for an H4 artifact error.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactErrorCode {
    /// A caller-provided value violated its domain grammar.
    InvalidValue,
    /// A bounded collection or document limit was exceeded.
    BoundExceeded,
    /// Two entries used the same canonical identity.
    Duplicate,
    /// Required evidence was absent.
    MissingEvidence,
    /// Bytes disagreed with a declared digest or binding.
    Integrity,
    /// A detached signature could not be verified.
    Signature,
    /// Independent build outputs disagreed.
    Reproducibility,
    /// Deterministic serialization failed.
    Serialization,
}

/// Typed H4 artifact failure with stable operation and recovery-safe detail.
#[derive(Debug, Error)]
#[error("{operation}: {detail}")]
pub struct ArtifactError {
    code: ArtifactErrorCode,
    operation: &'static str,
    detail: String,
    source: Option<serde_json::Error>,
}

impl ArtifactError {
    pub(crate) fn new(
        code: ArtifactErrorCode,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { code, operation, detail: detail.into(), source: None }
    }

    pub(crate) fn serialization(operation: &'static str, source: serde_json::Error) -> Self {
        Self {
            code: ArtifactErrorCode::Serialization,
            operation,
            detail: "canonical JSON serialization failed".to_owned(),
            source: Some(source),
        }
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn code(&self) -> ArtifactErrorCode {
        self.code
    }

    /// Returns the stable operation label.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns bounded, non-secret diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
