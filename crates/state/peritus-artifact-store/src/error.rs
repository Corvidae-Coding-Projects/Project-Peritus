//! Stable artifact-store errors and recovery guidance.

use std::{error::Error, fmt, io};

/// Stable machine-readable error codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Store configuration is invalid.
    InvalidConfiguration,
    /// A bounded textual metadata value is invalid.
    InvalidMetadata,
    /// A writer request is internally inconsistent or exceeds configured policy.
    InvalidWriteRequest,
    /// More bytes were supplied than the declared limit permits.
    ByteLimitExceeded,
    /// Checked byte accounting overflowed.
    ArithmeticOverflow,
    /// Final bytes do not have the expected size.
    SizeMismatch,
    /// Final bytes do not have the expected digest.
    DigestMismatch,
    /// Existing content does not match the digest encoded by its path.
    CorruptObject,
    /// A requested artifact is absent.
    MissingArtifact,
    /// A quota reservation would exceed its limit.
    QuotaExceeded,
    /// Physical storage cannot retain the configured free-space reserve for this allocation.
    StoragePressure,
    /// Another mutable artifact-store owner holds the canonical root.
    StoreOwned,
    /// A collection input or plan violates its state-machine contract.
    InvalidCollectionPlan,
    /// The artifact catalog is temporarily busy with another database owner.
    CatalogBusy,
    /// The artifact catalog is temporarily locked by shared-cache or schema ownership.
    CatalogLocked,
    /// The owner explicitly cancelled its catalog contention wait.
    CatalogWaitCancelled,
    /// `SQLite` denied access to the artifact catalog.
    CatalogPermissionDenied,
    /// The artifact catalog is read-only for the requested operation.
    CatalogReadOnly,
    /// The artifact catalog schema changed during the requested operation.
    CatalogSchemaChanged,
    /// `SQLite` reported malformed or non-database artifact catalog bytes.
    CatalogIntegrity,
    /// A filesystem operation failed.
    Io,
}

impl ErrorCode {
    /// Returns the compatibility-stable textual code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "artifact.invalid_configuration",
            Self::InvalidMetadata => "artifact.invalid_metadata",
            Self::InvalidWriteRequest => "artifact.invalid_write_request",
            Self::ByteLimitExceeded => "artifact.byte_limit_exceeded",
            Self::ArithmeticOverflow => "artifact.arithmetic_overflow",
            Self::SizeMismatch => "artifact.size_mismatch",
            Self::DigestMismatch => "artifact.digest_mismatch",
            Self::CorruptObject => "artifact.corrupt_object",
            Self::MissingArtifact => "artifact.missing",
            Self::QuotaExceeded => "artifact.quota_exceeded",
            Self::StoragePressure => "artifact.storage_pressure",
            Self::StoreOwned => "artifact.store_owned",
            Self::InvalidCollectionPlan => "artifact.invalid_collection_plan",
            Self::CatalogBusy => "artifact.catalog_busy",
            Self::CatalogLocked => "artifact.catalog_locked",
            Self::CatalogWaitCancelled => "artifact.catalog_wait_cancelled",
            Self::CatalogPermissionDenied => "artifact.catalog_permission_denied",
            Self::CatalogReadOnly => "artifact.catalog_read_only",
            Self::CatalogSchemaChanged => "artifact.catalog_schema_changed",
            Self::CatalogIntegrity => "artifact.catalog_integrity",
            Self::Io => "artifact.io",
        }
    }
}

/// Recommended action class for an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RecoveryClass {
    /// Correct the caller-supplied request before retrying.
    CorrectRequest,
    /// The operation may be retried without changing its identity.
    Retry,
    /// Startup recovery or operator cleanup must run before retrying.
    RecoverStore,
    /// Integrity is compromised; automatic retry must not hide the failure.
    TerminalIntegrity,
}

/// Narrow filesystem operation labels retained in I/O errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StoreOperation {
    /// Create or validate the store layout.
    Initialize,
    /// Canonicalize and validate a store path.
    Canonicalize,
    /// Create an exclusive temporary file.
    CreateTemporary,
    /// Stream bytes to a temporary file.
    WriteTemporary,
    /// Flush a temporary file.
    FlushTemporary,
    /// Synchronize a file or directory.
    Synchronize,
    /// Publish a temporary file to the object namespace.
    Publish,
    /// Inspect or hash an existing object.
    InspectObject,
    /// Create a kernel-owned temporary verification index.
    CreateVerificationIndex,
    /// Write authenticated nodes to a temporary verification index.
    WriteVerificationIndex,
    /// Read authenticated nodes from a temporary verification index.
    ReadVerificationIndex,
    /// Move an object into or out of quarantine.
    MoveQuarantine,
    /// Remove a temporary or quarantined file.
    Remove,
    /// Enumerate files during recovery.
    Recover,
    /// Observe filesystem capacity.
    ObserveSpace,
    /// Reserve physical filesystem space for a temporary artifact.
    ReserveSpace,
    /// Acquire exclusive mutable ownership of the canonical store root.
    AcquireOwnership,
    /// Access durable artifact catalog state.
    Catalog,
}

/// Phase at which a retained-free-space guarantee could not be satisfied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoragePressurePhase {
    /// Capacity was insufficient before the temporary allocation was attempted.
    BeforeReservation,
    /// The completed reservation left less space than the configured reserve.
    AfterReservation,
}

/// Exact observable cause of physical storage pressure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoragePressure {
    /// The filesystem observation could not satisfy the retained-space policy.
    InsufficientAvailableSpace {
        /// Exact temporary allocation requested by the writer.
        allocation_bytes: u64,
        /// Configured bytes that must remain available.
        minimum_free_bytes: u64,
        /// Bytes available to the process at the failed observation.
        available_bytes: u64,
        /// Whether the failure was observed before or after reservation.
        phase: StoragePressurePhase,
    },
    /// The operating system reported a physical or filesystem quota exhaustion.
    Filesystem {
        /// Filesystem operation that observed the exhaustion.
        operation: StoreOperation,
        /// Preserved operating-system error category.
        kind: io::ErrorKind,
    },
    /// `SQLite` reported that the filesystem containing durable catalog state is full.
    Catalog,
}

impl StoragePressure {
    /// Returns the exact available-byte threshold for a retained-space observation.
    ///
    /// Before reservation this includes the requested allocation. After reservation the
    /// allocation is already reflected by the filesystem, so only the retained reserve remains.
    #[must_use]
    pub const fn required_available_bytes(self) -> Option<u64> {
        match self {
            Self::InsufficientAvailableSpace {
                allocation_bytes,
                minimum_free_bytes,
                phase: StoragePressurePhase::BeforeReservation,
                ..
            } => allocation_bytes.checked_add(minimum_free_bytes),
            Self::InsufficientAvailableSpace {
                minimum_free_bytes,
                phase: StoragePressurePhase::AfterReservation,
                ..
            } => Some(minimum_free_bytes),
            Self::Filesystem { .. } | Self::Catalog => None,
        }
    }

    /// Returns the measured byte shortfall when the filesystem supplied exact capacity values.
    #[must_use]
    pub const fn additional_available_bytes_needed(self) -> Option<u64> {
        let Self::InsufficientAvailableSpace { available_bytes, .. } = self else {
            return None;
        };
        match self.required_available_bytes() {
            Some(required) => Some(required.saturating_sub(available_bytes)),
            None => None,
        }
    }
}

/// Preserved `SQLite` contention category for a catalog failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CatalogFailure {
    /// `SQLite` reported `SQLITE_BUSY`.
    Busy,
    /// `SQLite` reported `SQLITE_LOCKED`.
    Locked,
    /// `SQLite` reported `SQLITE_FULL`.
    StorageFull,
    /// `SQLite` reported `SQLITE_PERM` or statement authorization denial.
    PermissionDenied,
    /// `SQLite` reported `SQLITE_READONLY`.
    ReadOnly,
    /// `SQLite` reported `SQLITE_SCHEMA`.
    SchemaChanged,
    /// `SQLite` reported a malformed database image or a non-database file.
    Integrity,
    /// `SQLite` reported another catalog failure.
    Other,
}

/// Typed error returned by artifact-store operations.
#[derive(Debug)]
pub struct ArtifactStoreError {
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: ErrorDetail,
}

#[derive(Debug)]
enum ErrorDetail {
    Message(&'static str),
    Limit { attempted: u64, limit: u64 },
    Mismatch { expected: u64, actual: u64 },
    Capacity {
        allocation: u64,
        minimum_free: u64,
        available: u64,
        phase: StoragePressurePhase,
    },
    Io { operation: StoreOperation, source: io::Error },
    Catalog { failure: CatalogFailure, source: rusqlite::Error },
}

impl ArtifactStoreError {
    pub(crate) const fn message(
        code: ErrorCode,
        recovery: RecoveryClass,
        message: &'static str,
    ) -> Self {
        Self { code, recovery, detail: ErrorDetail::Message(message) }
    }

    pub(crate) const fn limit(code: ErrorCode, attempted: u64, limit: u64) -> Self {
        Self {
            code,
            recovery: RecoveryClass::CorrectRequest,
            detail: ErrorDetail::Limit { attempted, limit },
        }
    }

    pub(crate) const fn mismatch(code: ErrorCode, expected: u64, actual: u64) -> Self {
        Self {
            code,
            recovery: RecoveryClass::CorrectRequest,
            detail: ErrorDetail::Mismatch { expected, actual },
        }
    }

    pub(crate) const fn capacity(
        allocation: u64,
        minimum_free: u64,
        available: u64,
        phase: StoragePressurePhase,
    ) -> Self {
        Self {
            code: ErrorCode::StoragePressure,
            recovery: RecoveryClass::Retry,
            detail: ErrorDetail::Capacity {
                allocation,
                minimum_free,
                available,
                phase,
            },
        }
    }

    pub(crate) fn io(operation: StoreOperation, source: io::Error) -> Self {
        let code = match source.kind() {
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => {
                ErrorCode::StoragePressure
            }
            _ => ErrorCode::Io,
        };
        let recovery = match source.kind() {
            io::ErrorKind::PermissionDenied
            | io::ErrorKind::ReadOnlyFilesystem
            | io::ErrorKind::Unsupported => {
                RecoveryClass::RecoverStore
            }
            _ => RecoveryClass::Retry,
        };
        Self { code, recovery, detail: ErrorDetail::Io { operation, source } }
    }

    pub(crate) const fn storage_pressure_io(operation: StoreOperation, source: io::Error) -> Self {
        Self {
            code: ErrorCode::StoragePressure,
            recovery: RecoveryClass::Retry,
            detail: ErrorDetail::Io { operation, source },
        }
    }

    pub(crate) const fn store_owned(source: io::Error) -> Self {
        Self {
            code: ErrorCode::StoreOwned,
            recovery: RecoveryClass::Retry,
            detail: ErrorDetail::Io { operation: StoreOperation::AcquireOwnership, source },
        }
    }

    pub(crate) const fn catalog(
        code: ErrorCode,
        recovery: RecoveryClass,
        failure: CatalogFailure,
        source: rusqlite::Error,
    ) -> Self {
        Self { code, recovery, detail: ErrorDetail::Catalog { failure, source } }
    }

    /// Returns the stable error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the recommended recovery class.
    #[must_use]
    pub const fn recovery_class(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns the filesystem operation for an I/O error.
    #[must_use]
    pub const fn operation(&self) -> Option<StoreOperation> {
        match &self.detail {
            ErrorDetail::Io { operation, .. } => Some(*operation),
            ErrorDetail::Catalog { .. } => Some(StoreOperation::Catalog),
            _ => None,
        }
    }

    /// Returns the preserved `SQLite` catalog failure category, when applicable.
    #[must_use]
    pub const fn catalog_failure(&self) -> Option<CatalogFailure> {
        match &self.detail {
            ErrorDetail::Catalog { failure, .. } => Some(*failure),
            _ => None,
        }
    }

    /// Returns the exact physical-pressure observation, when this is a storage-pressure error.
    #[must_use]
    pub fn storage_pressure(&self) -> Option<StoragePressure> {
        match &self.detail {
            ErrorDetail::Capacity {
                allocation,
                minimum_free,
                available,
                phase,
            } => Some(StoragePressure::InsufficientAvailableSpace {
                allocation_bytes: *allocation,
                minimum_free_bytes: *minimum_free,
                available_bytes: *available,
                phase: *phase,
            }),
            ErrorDetail::Io { operation, source }
                if matches!(
                    source.kind(),
                    io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded
                ) =>
            {
                Some(StoragePressure::Filesystem {
                    operation: *operation,
                    kind: source.kind(),
                })
            }
            ErrorDetail::Catalog { failure: CatalogFailure::StorageFull, .. } => {
                Some(StoragePressure::Catalog)
            }
            ErrorDetail::Message(_)
            | ErrorDetail::Limit { .. }
            | ErrorDetail::Mismatch { .. }
            | ErrorDetail::Io { .. }
            | ErrorDetail::Catalog { .. } => None,
        }
    }
}

impl fmt::Display for ArtifactStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: ", self.code.as_str())?;
        match &self.detail {
            ErrorDetail::Message(message) => formatter.write_str(message),
            ErrorDetail::Limit { attempted, limit } => {
                write!(formatter, "attempted {attempted} bytes with limit {limit}")
            }
            ErrorDetail::Mismatch { expected, actual } => {
                write!(formatter, "expected {expected}, observed {actual}")
            }
            ErrorDetail::Capacity {
                allocation,
                minimum_free,
                available,
                phase,
            } => {
                write!(
                    formatter,
                    "allocation {allocation} bytes with retained reserve {minimum_free} observed {available} available bytes at {phase:?}"
                )
            }
            ErrorDetail::Io { operation, source } => write!(formatter, "{operation:?}: {source}"),
            ErrorDetail::Catalog { failure, source } => {
                write!(formatter, "Catalog {failure:?}: {source}")
            }
        }
    }
}

impl Error for ArtifactStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.detail {
            ErrorDetail::Io { source, .. } => Some(source),
            ErrorDetail::Catalog { source, .. } => Some(source),
            _ => None,
        }
    }
}
