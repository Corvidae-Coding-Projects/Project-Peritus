//! Stable network failures and recovery guidance.

use core::fmt;

/// Stable managed-network failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NetworkErrorKind {
    /// A value is malformed or exceeds a fixed bound.
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
    /// A stream read or write failed.
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
    /// Relay bounded bytes.
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

/// Bounded non-payload-bearing network error.
#[derive(Debug)]
pub struct NetworkError {
    kind: NetworkErrorKind,
    operation: NetworkOperation,
    recovery: RecoveryClass,
    detail: &'static str,
}

impl NetworkError {
    /// Creates one stable bounded failure.
    #[must_use]
    pub const fn new(
        kind: NetworkErrorKind,
        operation: NetworkOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail }
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
    /// Returns bounded safe detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
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
