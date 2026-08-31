//! Stable journal failures and recovery guidance.

use core::fmt;

/// Stable machine-actionable journal failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JournalErrorKind {
    /// A caller supplied a malformed identifier, frame, ordering, or bounded value.
    InvalidInput,
    /// An append contained no events.
    EmptyBatch,
    /// An input collection contained a duplicate identity.
    DuplicateIdentity,
    /// An input collection was not in canonical order.
    NonCanonicalOrder,
    /// An aggregate sequence or global position could not be represented.
    SequenceOverflow,
    /// An aggregate head compare-and-swap precondition was stale.
    StaleHead,
    /// A command identity was reused with a different request digest.
    IdempotencyConflict,
    /// A required finalized artifact was absent.
    MissingArtifact,
    /// The authority epoch compare-and-swap precondition was stale.
    StaleAuthorityEpoch,
    /// The credential-registry revision precondition was stale.
    StaleRegistry,
    /// A `SQLite` operation exhausted the configured busy timeout.
    Busy,
    /// The store is read-only.
    ReadOnly,
    /// A commit outcome could not be determined and must be resolved by command identity.
    IndeterminateCommit,
    /// Stored journal bytes, hashes, ordering, or heads are corrupt.
    CorruptJournal,
    /// The on-disk schema is not supported by this binary.
    UnsupportedSchema,
    /// A requested record does not exist.
    NotFound,
    /// An operating-system or `SQLite` failure occurred.
    Storage,
}

impl JournalErrorKind {
    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-JOURNAL-INPUT-001",
            Self::EmptyBatch => "PERITUS-JOURNAL-INPUT-002",
            Self::DuplicateIdentity => "PERITUS-JOURNAL-INPUT-003",
            Self::NonCanonicalOrder => "PERITUS-JOURNAL-INPUT-004",
            Self::SequenceOverflow => "PERITUS-JOURNAL-SEQUENCE-001",
            Self::StaleHead => "PERITUS-JOURNAL-CAS-001",
            Self::IdempotencyConflict => "PERITUS-JOURNAL-IDEMPOTENCY-001",
            Self::MissingArtifact => "PERITUS-JOURNAL-ARTIFACT-001",
            Self::StaleAuthorityEpoch => "PERITUS-JOURNAL-AUTHORITY-001",
            Self::StaleRegistry => "PERITUS-JOURNAL-REGISTRY-001",
            Self::Busy => "PERITUS-JOURNAL-SQLITE-001",
            Self::ReadOnly => "PERITUS-JOURNAL-SQLITE-002",
            Self::IndeterminateCommit => "PERITUS-JOURNAL-COMMIT-001",
            Self::CorruptJournal => "PERITUS-JOURNAL-INTEGRITY-001",
            Self::UnsupportedSchema => "PERITUS-JOURNAL-SCHEMA-001",
            Self::NotFound => "PERITUS-JOURNAL-QUERY-001",
            Self::Storage => "PERITUS-JOURNAL-STORAGE-001",
        }
    }
}

/// Recovery guidance for a journal failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// The caller may correct and resubmit the request.
    CallerCorrectable,
    /// Current durable state must be observed and a new plan produced.
    Reobserve,
    /// The same command identity and digest must be resolved before retrying.
    ResolveCommand,
    /// The operation may be retried after bounded backoff.
    Retry,
    /// The store requires operator repair or replacement.
    Terminal,
}

/// Typed journal failure retaining an optional `SQLite` source.
#[derive(Debug)]
pub struct JournalError {
    kind: JournalErrorKind,
    operation: &'static str,
    detail: &'static str,
    source: Option<rusqlite::Error>,
}

impl JournalError {
    pub(crate) const fn new(
        kind: JournalErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, detail, source: None }
    }

    pub(crate) fn sqlite(operation: &'static str, source: rusqlite::Error) -> Self {
        let kind = match &source {
            rusqlite::Error::SqliteFailure(error, _)
                if error.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                JournalErrorKind::Busy
            }
            rusqlite::Error::SqliteFailure(error, _)
                if error.code == rusqlite::ErrorCode::ReadOnly =>
            {
                JournalErrorKind::ReadOnly
            }
            _ => JournalErrorKind::Storage,
        };
        Self { kind, operation, detail: "SQLite operation failed", source: Some(source) }
    }

    pub(crate) const fn indeterminate(operation: &'static str, source: rusqlite::Error) -> Self {
        Self {
            kind: JournalErrorKind::IndeterminateCommit,
            operation,
            detail: "commit acknowledgement failed and command resolution was inconclusive",
            source: Some(source),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> JournalErrorKind {
        self.kind
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns the operation in which the failure occurred.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns recovery guidance for this failure.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        match self.kind {
            JournalErrorKind::InvalidInput
            | JournalErrorKind::EmptyBatch
            | JournalErrorKind::DuplicateIdentity
            | JournalErrorKind::NonCanonicalOrder => RecoveryClass::CallerCorrectable,
            JournalErrorKind::StaleHead
            | JournalErrorKind::StaleAuthorityEpoch
            | JournalErrorKind::StaleRegistry
            | JournalErrorKind::NotFound => RecoveryClass::Reobserve,
            JournalErrorKind::IndeterminateCommit => RecoveryClass::ResolveCommand,
            JournalErrorKind::Busy | JournalErrorKind::Storage => RecoveryClass::Retry,
            JournalErrorKind::SequenceOverflow
            | JournalErrorKind::IdempotencyConflict
            | JournalErrorKind::MissingArtifact
            | JournalErrorKind::ReadOnly
            | JournalErrorKind::CorruptJournal
            | JournalErrorKind::UnsupportedSchema => RecoveryClass::Terminal,
        }
    }

    /// Returns whether `SQLite` reported deterministic database or disk exhaustion.
    #[must_use]
    pub fn is_storage_exhausted(&self) -> bool {
        matches!(
            self.source.as_ref(),
            Some(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DiskFull
        )
    }
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {}: {}", self.code(), self.operation, self.detail)
    }
}

impl std::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source as _)
    }
}
