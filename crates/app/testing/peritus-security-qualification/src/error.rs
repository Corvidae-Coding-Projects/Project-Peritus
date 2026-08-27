//! Stable H0 campaign failures.

use thiserror::Error;

/// Stable qualification failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QualificationErrorCode {
    /// Caller input or a bounded domain value was invalid.
    InvalidInput,
    /// Closed catalog construction or mapping was inconsistent.
    Catalog,
    /// Evidence exceeded a count or byte bound.
    EvidenceBound,
    /// A fresh subject or adapter violated the runner protocol.
    SubjectProtocol,
    /// Native execution failed before a trustworthy observation was produced.
    NativeExecution,
    /// Cancellation was requested.
    Cancelled,
    /// Cleanup was incomplete or could not be observed.
    Cleanup,
    /// Canonical evidence serialization failed.
    Manifest,
    /// The verified policy rejected malformed canonical evidence.
    PolicyEvidence,
}

/// Stable corrective action associated with an H0 error.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QualificationRecovery {
    /// Correct caller-provided configuration or identity.
    CorrectInput,
    /// Provision a new disposable native subject.
    ReplaceSubject,
    /// Correct the native adapter and repeat the complete case.
    RepairAdapter,
    /// Rebuild the exact integrated candidate and its attestations.
    RebuildCandidate,
    /// Quarantine the run and retain its evidence for review.
    Quarantine,
    /// Resume with a new campaign after explicit cancellation.
    RestartCampaign,
}

/// Typed H0 qualification error with bounded safe detail.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{operation}: {detail}")]
pub struct QualificationError {
    code: QualificationErrorCode,
    recovery: QualificationRecovery,
    operation: &'static str,
    detail: String,
}

impl QualificationError {
    /// Creates a stable error, truncating detail at the UTF-8 boundary to avoid unbounded output.
    #[must_use]
    pub fn new(
        code: QualificationErrorCode,
        recovery: QualificationRecovery,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        if detail.len() > 1_024 {
            let mut boundary = 1_024;
            while !detail.is_char_boundary(boundary) {
                boundary -= 1;
            }
            detail.truncate(boundary);
        }
        Self { code, recovery, operation, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> QualificationErrorCode {
        self.code
    }

    /// Returns the prescribed recovery class.
    #[must_use]
    pub const fn recovery(&self) -> QualificationRecovery {
        self.recovery
    }

    /// Returns the stable operation label.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "private domain modules share this stable input-error constructor"
)]
pub(super) fn invalid(detail: impl Into<String>) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::InvalidInput,
        QualificationRecovery::CorrectInput,
        "validate H0 qualification input",
        detail,
    )
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "private runner and observation modules share this protocol-error constructor"
)]
pub(super) fn protocol(detail: impl Into<String>) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::SubjectProtocol,
        QualificationRecovery::RepairAdapter,
        "validate H0 subject observation",
        detail,
    )
}
