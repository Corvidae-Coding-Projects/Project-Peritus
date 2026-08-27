//! Stable H2 error classification.

use thiserror::Error;

/// Stable qualification failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QualificationErrorCode {
    /// An input contract was malformed or internally inconsistent.
    InvalidInput,
    /// A declared package artifact did not match its bytes.
    Integrity,
    /// A release layout or ownership boundary was unsafe.
    Layout,
    /// The target platform cannot provide a required facility.
    Unsupported,
    /// Evidence exceeded a closed H2 bound.
    EvidenceBound,
    /// A fresh subject violated the runner protocol.
    SubjectProtocol,
    /// A package lifecycle plan was incomplete or unsafe.
    Lifecycle,
    /// An operating-system file observation failed.
    FileObservation,
}

/// Stable corrective action associated with an H2 error.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QualificationRecovery {
    /// Correct the caller-provided value.
    CorrectInput,
    /// Rebuild the release from reviewed inputs.
    RebuildRelease,
    /// Provision a new clean qualification subject.
    ReplaceSubject,
    /// Configure the native host prerequisite and repeat qualification.
    ConfigureHost,
    /// Quarantine the observed package or subject evidence.
    Quarantine,
}

/// Typed, bounded-safe H2 qualification error.
#[derive(Debug, Error)]
#[error("{operation}: {detail}")]
pub struct QualificationError {
    code: QualificationErrorCode,
    recovery: QualificationRecovery,
    operation: &'static str,
    detail: String,
}

impl QualificationError {
    /// Creates a stable error without retaining platform-private source text.
    #[must_use]
    pub fn new(
        code: QualificationErrorCode,
        recovery: QualificationRecovery,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { code, recovery, operation, detail: detail.into() }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> QualificationErrorCode {
        self.code
    }

    /// Returns the expected corrective action.
    #[must_use]
    pub const fn recovery(&self) -> QualificationRecovery {
        self.recovery
    }

    /// Returns the stable operation label.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows bounded-safe diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
