//! Stable projection failure and recovery vocabulary.

use std::{error::Error, fmt};

/// Stable machine-actionable projection failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ProjectionErrorKind {
    /// A projection identity, version, or bound was invalid.
    InvalidInput,
    /// Global journal positions were not contiguous from one.
    PositionGap,
    /// A record was repeated or appeared behind the replay cursor.
    RecordOrder,
    /// An aggregate sequence or predecessor was inconsistent.
    AggregateOrder,
    /// The frame family is not in the frozen protocol registry.
    UnsupportedFamily,
    /// The family schema version is not supported.
    UnsupportedSchema,
    /// A frame payload failed its typed canonical decoder.
    InvalidFrame,
    /// A record attempted to change an aggregate's bound revision.
    StaleRevision,
    /// A pure fold invariant failed.
    FoldInvariant,
    /// A checkpoint did not bind the current journal and projection schema.
    StaleCheckpoint,
    /// Durable projection catalog bytes or metadata were corrupt.
    CorruptCatalog,
    /// A generation or active-pointer compare-and-swap lost a race.
    Conflict,
    /// `SQLite` could not complete the requested operation.
    Storage,
}

/// Stable operator response for a projection failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Retry the same operation after transient contention.
    Retry,
    /// Discard the projection generation and rebuild it from genesis.
    Rebuild,
    /// Repair authoritative journal data before replay can continue.
    RepairJournal,
    /// Correct a caller or deployment mismatch before retrying.
    CorrectInput,
}

/// Typed projection error with a stable kind and recovery class.
#[derive(Debug)]
pub struct ProjectionError {
    kind: ProjectionErrorKind,
    recovery: RecoveryClass,
    operation: &'static str,
    detail: String,
    source: Option<rusqlite::Error>,
}

impl ProjectionError {
    pub(crate) fn new(
        kind: ProjectionErrorKind,
        recovery: RecoveryClass,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, recovery, operation, detail: detail.into(), source: None }
    }

    pub(crate) fn sqlite(operation: &'static str, error: rusqlite::Error) -> Self {
        let recovery = match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                RecoveryClass::Retry
            }
            _ => RecoveryClass::CorrectInput,
        };
        Self {
            kind: ProjectionErrorKind::Storage,
            recovery,
            operation,
            detail: error.to_string(),
            source: Some(error),
        }
    }

    /// Returns the stable failure kind.
    #[must_use]
    pub const fn kind(&self) -> ProjectionErrorKind {
        self.kind
    }

    /// Returns the stable recovery classification.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl Error for ProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|source| source as &dyn Error)
    }
}
