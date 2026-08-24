//! Stable filesystem-tool failures and recovery guidance.

use core::fmt;

/// Stable filesystem-tool failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FsToolErrorKind {
    /// The structured input is invalid or outside a hard bound.
    InvalidInput,
    /// Immutable workspace inspection failed.
    Inspection,
    /// Patch compilation failed.
    Patch,
    /// Target-owned workspace authorization or mutation failed.
    Workspace,
    /// Tool protocol construction or rendering failed.
    Protocol,
    /// The requested operation is intentionally unsupported.
    Unsupported,
}

impl FsToolErrorKind {
    /// Returns the compatibility-stable code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-FS-TOOL-001",
            Self::Inspection => "PERITUS-FS-TOOL-002",
            Self::Patch => "PERITUS-FS-TOOL-003",
            Self::Workspace => "PERITUS-FS-TOOL-004",
            Self::Protocol => "PERITUS-FS-TOOL-005",
            Self::Unsupported => "PERITUS-FS-TOOL-006",
        }
    }
}

/// Filesystem tool operation associated with a failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FsToolOperation {
    /// Discover a bounded subtree.
    Discover,
    /// Observe one entry's metadata.
    Metadata,
    /// Read one bounded regular file.
    Read,
    /// Search bounded regular-file content.
    Search,
    /// Compile a create patch.
    Create,
    /// Compile a create-or-replace patch.
    Write,
    /// Compile a deletion patch.
    Remove,
    /// Compile an exact replacement patch.
    Replace,
    /// Compile a multi-file patch.
    Patch,
    /// Build or render the tool catalog.
    Catalog,
}

/// Required response to a filesystem-tool failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct structured input before retrying.
    CorrectInput,
    /// Re-open and re-observe the immutable workspace.
    Reobserve,
    /// Obtain fresh authority for the exact current workspace version.
    Reauthorize,
    /// Reconcile an indeterminate workspace transaction.
    Reconcile,
    /// Select a supported tool operation.
    SelectSupportedOperation,
}

/// Bounded typed filesystem-tool error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FsToolError {
    kind: FsToolErrorKind,
    operation: FsToolOperation,
    recovery: RecoveryClass,
    detail: &'static str,
}

impl FsToolError {
    pub(crate) const fn new(
        kind: FsToolErrorKind,
        operation: FsToolOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail }
    }

    pub(crate) const fn invalid(operation: FsToolOperation, detail: &'static str) -> Self {
        Self::new(FsToolErrorKind::InvalidInput, operation, RecoveryClass::CorrectInput, detail)
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(&self) -> FsToolErrorKind {
        self.kind
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> FsToolOperation {
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

impl fmt::Display for FsToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.kind.code(), self.operation, self.detail)
    }
}

impl std::error::Error for FsToolError {}
