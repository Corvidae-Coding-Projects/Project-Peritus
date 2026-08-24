//! Stable Git-tool failures and recovery guidance.

use core::fmt;

/// Stable Git-tool failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitToolErrorKind {
    /// Structured input is invalid or exceeds a hard bound.
    InvalidInput,
    /// Structured C1 Git observation failed.
    Git,
    /// Target-owned workspace authorization or effect failed.
    Workspace,
    /// Protocol catalog or rendering construction failed.
    Protocol,
    /// No lower authorized operation owns the requested effect.
    Unsupported,
}

impl GitToolErrorKind {
    /// Returns the compatibility-stable error code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-GIT-TOOL-001",
            Self::Git => "PERITUS-GIT-TOOL-002",
            Self::Workspace => "PERITUS-GIT-TOOL-003",
            Self::Protocol => "PERITUS-GIT-TOOL-004",
            Self::Unsupported => "PERITUS-GIT-TOOL-005",
        }
    }
}

/// Git tool operation associated with a failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitToolOperation {
    /// Observe structured status.
    Status,
    /// Observe an immutable commit diff.
    Diff,
    /// Observe bounded history.
    History,
    /// Create a candidate and retained snapshot.
    Candidate,
    /// Inspect current or retained snapshot metadata.
    Snapshot,
    /// Restore a retained snapshot as a successor.
    Rollback,
    /// Deliver an approved merge to a user branch.
    Merge,
    /// Build or render the descriptor catalog.
    Catalog,
}

/// Required caller response to a Git-tool failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct structured input before retrying.
    CorrectInput,
    /// Re-observe the immutable repository/workspace.
    Reobserve,
    /// Obtain fresh exact authority.
    Reauthorize,
    /// Reconcile a dirty or indeterminate workspace.
    Reconcile,
    /// Select an operation supported by the lower boundary.
    SelectSupportedOperation,
}

/// Bounded typed Git-tool error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitToolError {
    kind: GitToolErrorKind,
    operation: GitToolOperation,
    recovery: RecoveryClass,
    detail: &'static str,
}

impl GitToolError {
    pub(crate) const fn new(
        kind: GitToolErrorKind,
        operation: GitToolOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail }
    }

    pub(crate) const fn invalid(operation: GitToolOperation, detail: &'static str) -> Self {
        Self::new(GitToolErrorKind::InvalidInput, operation, RecoveryClass::CorrectInput, detail)
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(&self) -> GitToolErrorKind {
        self.kind
    }
    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> GitToolOperation {
        self.operation
    }
    /// Returns the required recovery family.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
    /// Returns bounded content-free detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for GitToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.kind.code(), self.operation, self.detail)
    }
}

impl std::error::Error for GitToolError {}
