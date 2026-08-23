//! Stable workspace failures and recovery guidance.

use core::fmt;

/// Stable machine-readable failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCode {
    /// Input is outside the checked workspace contract.
    InvalidInput,
    /// The workspace or target resource identity differs.
    ResourceMismatch,
    /// B0, B1, or C0 authorization facts differ.
    AuthorizationMismatch,
    /// The durable B0 transition is not the exact action dispatch.
    MissingDispatch,
    /// A receipt was already consumed by this gateway.
    ReceiptReused,
    /// The workspace generation or logical revision is stale.
    StaleWorkspace,
    /// The committed lease is stale, inactive, or expired.
    StaleLease,
    /// A writable operation was requested from an unsafe state.
    WorkspaceUnavailable,
    /// The checked patch adapter rejected or could not apply a transaction.
    Patch,
    /// The structured Git adapter failed.
    Git,
    /// Manifest finalization failed after an effect boundary.
    Artifact,
    /// Complete inspection found a known divergence.
    Dirty,
    /// Inspection could not establish the complete state.
    Indeterminate,
    /// A one-based state counter cannot advance.
    RevisionExhausted,
}

impl ErrorCode {
    /// Returns the stable external code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-WORKSPACE-001",
            Self::ResourceMismatch => "PERITUS-WORKSPACE-002",
            Self::AuthorizationMismatch => "PERITUS-WORKSPACE-003",
            Self::MissingDispatch => "PERITUS-WORKSPACE-004",
            Self::ReceiptReused => "PERITUS-WORKSPACE-005",
            Self::StaleWorkspace => "PERITUS-WORKSPACE-006",
            Self::StaleLease => "PERITUS-WORKSPACE-007",
            Self::WorkspaceUnavailable => "PERITUS-WORKSPACE-008",
            Self::Patch => "PERITUS-WORKSPACE-009",
            Self::Git => "PERITUS-WORKSPACE-010",
            Self::Artifact => "PERITUS-WORKSPACE-011",
            Self::Dirty => "PERITUS-WORKSPACE-012",
            Self::Indeterminate => "PERITUS-WORKSPACE-013",
            Self::RevisionExhausted => "PERITUS-WORKSPACE-014",
        }
    }
}

/// Operation in progress when a failure was observed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkspaceOperation {
    /// Open a checked workspace handle.
    Open,
    /// Validate committed authority.
    Authorize,
    /// Apply an atomic patch.
    Mutate,
    /// Create and retain a candidate snapshot.
    Candidate,
    /// Restore a retained snapshot as a successor revision.
    Rollback,
    /// Finalize a content-addressed manifest.
    FinalizeManifest,
    /// Inspect state after restart or fencing.
    Reconcile,
}

/// Stable caller recovery category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct the request before retrying.
    CorrectRequest,
    /// Obtain fresh B0/B1/C0 observations and authorize again.
    Reauthorize,
    /// Re-open and inspect the workspace state.
    Reobserve,
    /// Run explicit transaction and Git reconciliation.
    Reconcile,
    /// Quarantine the workspace pending operator action.
    Quarantine,
}

/// Bounded typed workspace failure.
#[derive(Debug)]
pub struct WorkspaceError {
    code: ErrorCode,
    operation: WorkspaceOperation,
    recovery: RecoveryClass,
    detail: &'static str,
}

impl WorkspaceError {
    pub(crate) const fn new(
        code: ErrorCode,
        operation: WorkspaceOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { code, operation, recovery, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }
    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> WorkspaceOperation {
        self.operation
    }
    /// Returns the required recovery family.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
    /// Returns bounded non-content-bearing context.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.code.as_str(), self.operation, self.detail)
    }
}

impl std::error::Error for WorkspaceError {}

pub const fn mismatch(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::AuthorizationMismatch,
        WorkspaceOperation::Authorize,
        RecoveryClass::Reauthorize,
        detail,
    )
}
