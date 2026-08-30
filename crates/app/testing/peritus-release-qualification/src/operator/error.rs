//! Typed failures from the H4 native evidence operator.

use std::path::Path;

/// Stable H4 operator failure class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorErrorCode {
    /// Command-line arguments did not match the closed grammar.
    Argument,
    /// A filesystem operation failed or named an unsafe file type.
    Io,
    /// Evidence bytes or signature material failed validation.
    Integrity,
    /// A validated output already exists.
    OutputExists,
    /// Final qualification completed and retained a `NotReady` report.
    NotReady,
}

/// One human-readable H4 operator failure with a stable class.
#[derive(Debug, thiserror::Error)]
#[error("{code:?}: {detail}")]
pub struct OperatorError {
    code: OperatorErrorCode,
    detail: String,
}

impl OperatorError {
    pub(super) fn usage() -> Self {
        Self::argument(
            "usage: peritus-h4 <envelope|verify> --binding FILE --kind KIND \
             --disposition <satisfied|not-satisfied> --retained-path PATH --payload FILE \
             --output FILE [--key-id ID --public-key FILE --signature FILE]; or \
             peritus-h4 finalize --plan FILE --evidence-root DIR --output DIR",
        )
    }

    pub(super) fn argument(detail: impl Into<String>) -> Self {
        Self { code: OperatorErrorCode::Argument, detail: detail.into() }
    }

    pub(super) fn io(operation: &str, path: &Path, source: &std::io::Error) -> Self {
        Self {
            code: if source.kind() == std::io::ErrorKind::AlreadyExists {
                OperatorErrorCode::OutputExists
            } else {
                OperatorErrorCode::Io
            },
            detail: format!("{operation} {}: {source}", path.display()),
        }
    }

    pub(super) fn integrity(detail: impl Into<String>) -> Self {
        Self { code: OperatorErrorCode::Integrity, detail: detail.into() }
    }

    pub(super) fn not_ready(detail: impl Into<String>) -> Self {
        Self { code: OperatorErrorCode::NotReady, detail: detail.into() }
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn code(&self) -> OperatorErrorCode {
        self.code
    }
}

impl From<serde_json::Error> for OperatorError {
    fn from(source: serde_json::Error) -> Self {
        Self::integrity(format!("invalid canonical JSON: {source}"))
    }
}

impl From<peritus_release_artifacts::ArtifactError> for OperatorError {
    fn from(source: peritus_release_artifacts::ArtifactError) -> Self {
        Self::integrity(source.to_string())
    }
}

impl From<crate::QualificationError> for OperatorError {
    fn from(source: crate::QualificationError) -> Self {
        Self::integrity(source.to_string())
    }
}
