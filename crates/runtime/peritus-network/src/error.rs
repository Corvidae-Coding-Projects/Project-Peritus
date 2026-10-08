//! Stable network failures and recovery guidance.

use core::fmt;

/// Stable managed-network failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NetworkErrorKind {
    /// A value is malformed or cannot be represented.
    InvalidInput,
    /// A request is outside the checked network plan.
    Denied,
    /// DNS resolution failed or returned unusable answers.
    Resolution,
    /// A redirect is denied or exceeds its bound.
    Redirect,
    /// The managed proxy cannot bind or accept.
    Proxy,
    /// The upstream connection failed.
    Connect,
    /// A stream or private proxy storage operation failed.
    Io,
    /// A byte, duration, connection, or worker ceiling was crossed.
    Limit,
    /// A routing or upstream credential is missing, expired, or mismatched.
    Credential,
    /// Shutdown could not prove that owned work joined.
    IncompleteTeardown,
    /// A persisted runtime record is malformed or mismatched.
    Recovery,
}

impl NetworkErrorKind {
    /// Returns the stable machine-readable code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-NETWORK-001",
            Self::Denied => "PERITUS-NETWORK-002",
            Self::Resolution => "PERITUS-NETWORK-003",
            Self::Redirect => "PERITUS-NETWORK-004",
            Self::Proxy => "PERITUS-NETWORK-005",
            Self::Connect => "PERITUS-NETWORK-006",
            Self::Io => "PERITUS-NETWORK-007",
            Self::Limit => "PERITUS-NETWORK-008",
            Self::Credential => "PERITUS-NETWORK-009",
            Self::IncompleteTeardown => "PERITUS-NETWORK-010",
            Self::Recovery => "PERITUS-NETWORK-011",
        }
    }
}

/// Operation active when the error was observed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NetworkOperation {
    /// Compile a runtime plan.
    Compile,
    /// Match a requested destination.
    Match,
    /// Resolve a DNS name.
    Resolve,
    /// Validate a redirect.
    Redirect,
    /// Bind or run the proxy.
    Proxy,
    /// Connect to an admitted upstream.
    Connect,
    /// Relay bytes through an owned connection.
    Relay,
    /// Acquire or inject a credential.
    Credential,
    /// Stop and join proxy work.
    Shutdown,
    /// Reopen a runtime record.
    Recover,
}

/// Recommended recovery family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct the request or bounds.
    CorrectRequest,
    /// Replan against current checked policy.
    Replan,
    /// Retry a transient host operation.
    Retry,
    /// Reacquire the exact credential lease.
    ReacquireCredential,
    /// Cancel the owner and join all work.
    CancelAndJoin,
    /// Reopen and reconcile persisted state.
    Reconcile,
}

/// Non-payload-bearing network error with stable static detail.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct NetworkError {
    kind: NetworkErrorKind,
    operation: NetworkOperation,
    recovery: RecoveryClass,
    detail: &'static str,
    storage: bool,
}

impl NetworkError {
    /// Creates one stable failure.
    #[must_use]
    pub const fn new(
        kind: NetworkErrorKind,
        operation: NetworkOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail, storage: false }
    }
    #[must_use]
    pub(crate) const fn storage(operation: NetworkOperation, detail: &'static str) -> Self {
        Self {
            kind: NetworkErrorKind::Io,
            operation,
            recovery: RecoveryClass::CancelAndJoin,
            detail,
            storage: true,
        }
    }
    #[must_use]
    pub(crate) const fn storage_after_limit(
        operation: NetworkOperation,
        detail: &'static str,
    ) -> Self {
        Self {
            kind: NetworkErrorKind::Limit,
            operation,
            recovery: RecoveryClass::CancelAndJoin,
            detail,
            storage: true,
        }
    }
    #[must_use]
    pub(crate) const fn is_storage(&self) -> bool {
        self.storage
    }
    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> NetworkErrorKind {
        self.kind
    }
    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> NetworkOperation {
        self.operation
    }
    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
    /// Returns static safe detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Debug for NetworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkError")
            .field("kind", &self.kind)
            .field("operation", &self.operation)
            .field("recovery", &self.recovery)
            .field("detail", &self.detail)
            .finish()
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.kind.code(), self.operation, self.detail)
    }
}

impl std::error::Error for NetworkError {}

pub const fn invalid(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::InvalidInput,
        NetworkOperation::Compile,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

pub const fn denied(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Denied,
        NetworkOperation::Match,
        RecoveryClass::Replan,
        detail,
    )
}
