//! Stable daemon failures and recovery instructions.

use core::fmt;

/// Stable daemon failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DaemonErrorCode {
    /// Configuration or a caller value is invalid.
    InvalidInput,
    /// A second live owner already holds the daemon identity.
    AlreadyRunning,
    /// IPC authentication or application authority was denied.
    Unauthorized,
    /// The current readiness phase does not admit the operation.
    NotReady,
    /// A bounded queue or resource ceiling was reached.
    ResourceLimit,
    /// Durable state is corrupt or internally inconsistent.
    CorruptState,
    /// A migration or durable command must be reconciled.
    RecoveryRequired,
    /// A platform capability required by this configuration is unavailable.
    Unsupported,
    /// A storage operation failed.
    Storage,
    /// A local transport operation failed.
    Transport,
    /// An owned task failed or terminated unexpectedly.
    Worker,
    /// Shutdown completed with exact remaining work.
    UncleanShutdown,
}

impl DaemonErrorCode {
    /// Returns the stable machine-readable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-DAEMON-INPUT-001",
            Self::AlreadyRunning => "PERITUS-DAEMON-INSTANCE-001",
            Self::Unauthorized => "PERITUS-DAEMON-AUTH-001",
            Self::NotReady => "PERITUS-DAEMON-READY-001",
            Self::ResourceLimit => "PERITUS-DAEMON-LIMIT-001",
            Self::CorruptState => "PERITUS-DAEMON-STATE-001",
            Self::RecoveryRequired => "PERITUS-DAEMON-RECOVERY-001",
            Self::Unsupported => "PERITUS-DAEMON-PLATFORM-001",
            Self::Storage => "PERITUS-DAEMON-STORAGE-001",
            Self::Transport => "PERITUS-DAEMON-IPC-001",
            Self::Worker => "PERITUS-DAEMON-WORKER-001",
            Self::UncleanShutdown => "PERITUS-DAEMON-SHUTDOWN-001",
        }
    }
}

/// Stable recovery instruction for one daemon failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DaemonRecovery {
    /// Correct configuration or request data before retrying.
    CorrectRequest,
    /// Retry after bounded backoff without changing identity.
    Retry,
    /// Reopen durable state and reconcile exact recorded identities.
    Reconcile,
    /// Continue in explicitly read-only diagnostic mode.
    ReadOnly,
    /// Stop and request operator intervention.
    Operator,
}

/// Typed daemon failure with stable classification and optional source.
#[derive(Debug)]
pub struct DaemonError {
    code: DaemonErrorCode,
    recovery: DaemonRecovery,
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl DaemonError {
    /// Creates a source-free typed failure.
    #[must_use]
    pub fn new(
        code: DaemonErrorCode,
        recovery: DaemonRecovery,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { code, recovery, operation, detail: detail.into(), source: None }
    }

    /// Creates a typed failure retaining its source.
    pub fn with_source(
        code: DaemonErrorCode,
        recovery: DaemonRecovery,
        operation: &'static str,
        detail: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self { code, recovery, operation, detail: detail.into(), source: Some(Box::new(source)) }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn code_kind(&self) -> DaemonErrorCode {
        self.code
    }
    /// Returns the stable diagnostic string.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code.code()
    }
    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> DaemonRecovery {
        self.recovery
    }
    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Borrows inert bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for DaemonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {}: {}", self.code(), self.operation, self.detail)
    }
}

impl std::error::Error for DaemonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as _)
    }
}
