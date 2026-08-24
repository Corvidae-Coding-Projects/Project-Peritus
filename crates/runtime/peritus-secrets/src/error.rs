//! Stable secret failures with non-content-bearing diagnostics.

use core::fmt;

/// Stable secret failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretErrorKind {
    /// A checked value is invalid or outside a fixed bound.
    InvalidInput,
    /// The exact referenced secret does not exist.
    Missing,
    /// The platform credential store is locked.
    Locked,
    /// Access was denied.
    Denied,
    /// The requested version is stale or differs.
    StaleVersion,
    /// No truthful credential-store adapter is available.
    Unavailable,
    /// Stored material is malformed.
    Corrupt,
    /// The exact lease was expired, exhausted, or revoked.
    Revoked,
    /// Delivery to the exact destination failed.
    Delivery,
    /// Release could not establish complete cleanup.
    Cleanup,
    /// A platform or filesystem operation failed.
    Io,
    /// A recovery record is malformed or mismatched.
    Recovery,
}

impl SecretErrorKind {
    /// Returns the stable machine-readable code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-SECRETS-001",
            Self::Missing => "PERITUS-SECRETS-002",
            Self::Locked => "PERITUS-SECRETS-003",
            Self::Denied => "PERITUS-SECRETS-004",
            Self::StaleVersion => "PERITUS-SECRETS-005",
            Self::Unavailable => "PERITUS-SECRETS-006",
            Self::Corrupt => "PERITUS-SECRETS-007",
            Self::Revoked => "PERITUS-SECRETS-008",
            Self::Delivery => "PERITUS-SECRETS-009",
            Self::Cleanup => "PERITUS-SECRETS-010",
            Self::Io => "PERITUS-SECRETS-011",
            Self::Recovery => "PERITUS-SECRETS-012",
        }
    }
}

/// Operation in progress when a secret failure occurred.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretOperation {
    /// Validate a secret request or lease.
    Validate,
    /// Probe the configured store.
    Probe,
    /// Resolve exact material.
    Lookup,
    /// Create a lease.
    Lease,
    /// Deliver material.
    Deliver,
    /// Revoke a lease.
    Revoke,
    /// Remove delivery artifacts and zeroize buffers.
    Cleanup,
    /// Reopen a recovery record.
    Recover,
}

/// Recommended recovery family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct the request.
    CorrectRequest,
    /// Unlock or authorize the credential service.
    UnlockStore,
    /// Obtain an exact current reference and lease.
    Reacquire,
    /// Retry a transient platform operation.
    Retry,
    /// Revoke and clean every partial delivery.
    RevokeAndClean,
    /// Reconcile the persisted runtime record.
    Reconcile,
}

/// Bounded secret error that never owns secret material.
#[derive(Debug)]
pub struct SecretError {
    kind: SecretErrorKind,
    operation: SecretOperation,
    recovery: RecoveryClass,
    detail: &'static str,
}

impl SecretError {
    /// Creates a stable non-content-bearing error.
    #[must_use]
    pub const fn new(
        kind: SecretErrorKind,
        operation: SecretOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail }
    }
    /// Returns the category.
    #[must_use]
    pub const fn kind(&self) -> SecretErrorKind {
        self.kind
    }
    /// Returns the operation.
    #[must_use]
    pub const fn operation(&self) -> SecretOperation {
        self.operation
    }
    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
    /// Returns bounded safe detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for SecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.kind.code(), self.operation, self.detail)
    }
}

impl std::error::Error for SecretError {}

pub const fn invalid(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::InvalidInput,
        SecretOperation::Validate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
