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
    /// A collection input or plan violates its state-machine contract.
    InvalidCollectionPlan,
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
            Self::InvalidCollectionPlan => "artifact.invalid_collection_plan",
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
    /// Move an object into or out of quarantine.
    MoveQuarantine,
    /// Remove a temporary or quarantined file.
    Remove,
    /// Enumerate files during recovery.
    Recover,
    /// Observe filesystem capacity.
    ObserveSpace,
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
    Io { operation: StoreOperation, source: io::Error },
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

    pub(crate) fn io(operation: StoreOperation, source: io::Error) -> Self {
        let recovery = match source.kind() {
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
                RecoveryClass::RecoverStore
            }
            _ => RecoveryClass::Retry,
        };
        Self { code: ErrorCode::Io, recovery, detail: ErrorDetail::Io { operation, source } }
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
            _ => None,
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
            ErrorDetail::Io { operation, source } => write!(formatter, "{operation:?}: {source}"),
        }
    }
}

impl Error for ArtifactStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.detail {
            ErrorDetail::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
