//! Stable SDK error categories.

use std::{error::Error, fmt};

/// Closed error category for manifest, payload, and wire failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SdkErrorKind {
    /// Text or identifier syntax is invalid.
    InvalidIdentity,
    /// A semantic version is invalid.
    InvalidVersion,
    /// Manifest syntax or invariants are invalid.
    InvalidManifest,
    /// A canonical collection is duplicated or unsorted.
    NonCanonical,
    /// A configured or observed limit was exceeded.
    LimitExceeded,
    /// JSON syntax or supported value shape is invalid.
    InvalidJson,
    /// Frame length or content is malformed.
    InvalidFrame,
    /// Protocol versions have no compatible value.
    IncompatibleProtocol,
}

/// Typed SDK error with stable operation and bounded diagnostic text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkError {
    kind: SdkErrorKind,
    operation: &'static str,
    detail: String,
}

impl SdkError {
    /// Creates a typed SDK error.
    #[must_use]
    pub fn new(kind: SdkErrorKind, operation: &'static str, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        detail.truncate(512);
        Self { kind, operation, detail }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> SdkErrorKind {
        self.kind
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows the bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl Error for SdkError {}
