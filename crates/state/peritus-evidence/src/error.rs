//! Stable evidence failures and operator recovery actions.

use std::{error::Error, fmt};

/// Stable evidence failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EvidenceErrorKind {
    /// An identity, tag, ordering, or configured bound is invalid.
    InvalidInput,
    /// The named journal record or command batch does not exist.
    MissingJournalRecord,
    /// Checked export and durable journal provenance disagree.
    JournalMismatch,
    /// The record revision does not match its journal binding.
    RevisionMismatch,
    /// A referenced artifact is absent or not referenceable.
    MissingArtifact,
    /// Artifact metadata or bytes disagree with their digest.
    CorruptArtifact,
    /// A causal parent is absent, reordered, cyclic, or not older.
    InvalidCause,
    /// The evidence identity already names a different record.
    IdentityConflict,
    /// The record is explicitly invalidated or revision-stale.
    StaleEvidence,
    /// Durable evidence rows violate their canonical schema.
    CorruptCatalog,
    /// Bundle bytes are malformed, truncated, reordered, or digest-invalid.
    InvalidBundle,
    /// `SQLite` could not complete an operation.
    Storage,
    /// Filesystem streaming failed.
    Io,
    /// Checked arithmetic exhausted its range.
    ArithmeticOverflow,
}

/// Stable action callers should take after a failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryAction {
    /// Correct the supplied record, manifest, or bundle.
    CorrectInput,
    /// Retry after bounded environmental contention.
    Retry,
    /// Repair authoritative journal or artifact state before retrying.
    RepairDependency,
    /// Rebuild the evidence catalog from retained immutable sources.
    RebuildCatalog,
    /// Treat the evidence as permanently stale and obtain a fresh observation.
    ObtainFreshEvidence,
}

#[derive(Debug)]
enum Source {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Artifact(peritus_artifact_store::ArtifactStoreError),
}

/// Typed evidence error with stable failure and recovery classifications.
#[derive(Debug)]
pub struct EvidenceError {
    kind: EvidenceErrorKind,
    recovery: RecoveryAction,
    operation: &'static str,
    detail: String,
    source: Option<Source>,
}

impl EvidenceError {
    pub(crate) fn new(
        kind: EvidenceErrorKind,
        recovery: RecoveryAction,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, recovery, operation, detail: detail.into(), source: None }
    }

    pub(crate) fn sqlite(operation: &'static str, error: rusqlite::Error) -> Self {
        let recovery = match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                RecoveryAction::Retry
            }
            _ => RecoveryAction::RepairDependency,
        };
        Self {
            kind: EvidenceErrorKind::Storage,
            recovery,
            operation,
            detail: error.to_string(),
            source: Some(Source::Sqlite(error)),
        }
    }

    pub(crate) fn io(operation: &'static str, error: std::io::Error) -> Self {
        Self {
            kind: EvidenceErrorKind::Io,
            recovery: RecoveryAction::Retry,
            operation,
            detail: error.to_string(),
            source: Some(Source::Io(error)),
        }
    }

    pub(crate) fn artifact(
        operation: &'static str,
        error: peritus_artifact_store::ArtifactStoreError,
    ) -> Self {
        let kind = match error.code() {
            peritus_artifact_store::ErrorCode::MissingArtifact => {
                EvidenceErrorKind::MissingArtifact
            }
            peritus_artifact_store::ErrorCode::Io => EvidenceErrorKind::Io,
            _ => EvidenceErrorKind::CorruptArtifact,
        };
        let recovery = match error.recovery_class() {
            peritus_artifact_store::RecoveryClass::Retry => RecoveryAction::Retry,
            _ => RecoveryAction::RepairDependency,
        };
        Self {
            kind,
            recovery,
            operation,
            detail: error.to_string(),
            source: Some(Source::Artifact(error)),
        }
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(&self) -> EvidenceErrorKind {
        self.kind
    }

    /// Returns the stable recovery action.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryAction {
        self.recovery
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows the diagnostic detail without changing the stable classification contract.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl Error for EvidenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|source| match source {
            Source::Sqlite(error) => error as &dyn Error,
            Source::Io(error) => error as &dyn Error,
            Source::Artifact(error) => error as &dyn Error,
        })
    }
}
