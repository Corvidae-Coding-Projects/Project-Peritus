//! Stable redaction-safe trace failure vocabulary.

use core::fmt;
use std::error::Error;

/// Stable trace failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TraceErrorKind {
    /// A trace or span identity is invalid.
    InvalidIdentity,
    /// A causal entity binding is structurally invalid.
    InvalidBinding,
    /// An observation collection or field exceeds a fixed bound.
    LimitExceeded,
    /// A sorted canonical collection is duplicated or out of order.
    NonCanonical,
    /// A span or trace event sequence is invalid or overflowed.
    Sequence,
    /// A structural or event-level causal predecessor is missing or inconsistent.
    CausalIntegrity,
    /// An event identity was reused with changed canonical bytes.
    DuplicateConflict,
    /// A span lifecycle transition is invalid.
    InvalidTransition,
    /// Caller-observed time regressed within a span.
    TimeRegression,
    /// Sensitive content was not safely omitted or vault-bound.
    Redaction,
    /// A canonical observation frame is malformed or mismatches its journal envelope.
    InvalidFrame,
    /// C0 persistence rejected or could not complete the operation.
    Storage,
    /// Projection replay or shadow rebuilding failed.
    Projection,
    /// Authoritative journal history failed integrity validation.
    Integrity,
}

impl TraceErrorKind {
    /// Returns a compatibility-stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidIdentity => "PERITUS-TRACE-IDENTITY-001",
            Self::InvalidBinding => "PERITUS-TRACE-BINDING-001",
            Self::LimitExceeded => "PERITUS-TRACE-LIMIT-001",
            Self::NonCanonical => "PERITUS-TRACE-CANONICAL-001",
            Self::Sequence => "PERITUS-TRACE-SEQUENCE-001",
            Self::CausalIntegrity => "PERITUS-TRACE-CAUSAL-001",
            Self::DuplicateConflict => "PERITUS-TRACE-DUPLICATE-001",
            Self::InvalidTransition => "PERITUS-TRACE-TRANSITION-001",
            Self::TimeRegression => "PERITUS-TRACE-TIME-001",
            Self::Redaction => "PERITUS-TRACE-REDACTION-001",
            Self::InvalidFrame => "PERITUS-TRACE-FRAME-001",
            Self::Storage => "PERITUS-TRACE-STORAGE-001",
            Self::Projection => "PERITUS-TRACE-PROJECTION-001",
            Self::Integrity => "PERITUS-TRACE-INTEGRITY-001",
        }
    }
}

/// Stable recovery guidance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct the observation before retrying.
    CorrectInput,
    /// Reobserve the durable trace head and plan again.
    Reobserve,
    /// Resolve the same C0 command identity before attempting new work.
    ResolveCommand,
    /// Rebuild the replaceable trace projection from checked journal history.
    RebuildProjection,
    /// Retry after bounded backoff without changing observation identity.
    Retry,
    /// Authoritative data requires operator repair.
    TerminalIntegrity,
}

/// Typed trace error whose default formatting never includes observation or sensitive content.
pub struct TraceError {
    kind: TraceErrorKind,
    recovery: RecoveryClass,
    operation: &'static str,
    detail: &'static str,
    source_class: Option<&'static str>,
}

impl TraceError {
    pub(crate) const fn static_error(
        kind: TraceErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self { kind, recovery: recovery(kind), operation, detail, source_class: None }
    }

    pub(crate) const fn journal(
        operation: &'static str,
        source: &peritus_journal::JournalError,
    ) -> Self {
        let recovery = match source.recovery() {
            peritus_journal::RecoveryClass::CallerCorrectable => RecoveryClass::CorrectInput,
            peritus_journal::RecoveryClass::Reobserve => RecoveryClass::Reobserve,
            peritus_journal::RecoveryClass::ResolveCommand => RecoveryClass::ResolveCommand,
            peritus_journal::RecoveryClass::Retry => RecoveryClass::Retry,
            peritus_journal::RecoveryClass::Terminal => RecoveryClass::TerminalIntegrity,
        };
        Self {
            kind: TraceErrorKind::Storage,
            recovery,
            operation,
            detail: "C0 journal operation failed",
            source_class: Some("journal"),
        }
    }

    pub(crate) const fn codec(operation: &'static str) -> Self {
        Self {
            kind: TraceErrorKind::InvalidFrame,
            recovery: RecoveryClass::CorrectInput,
            operation,
            detail: "canonical trace frame validation failed",
            source_class: Some("codec"),
        }
    }

    pub(crate) const fn projection(
        operation: &'static str,
        source: &peritus_projection::ProjectionError,
    ) -> Self {
        let recovery = match source.recovery() {
            peritus_projection::RecoveryClass::Retry => RecoveryClass::Retry,
            peritus_projection::RecoveryClass::Rebuild => RecoveryClass::RebuildProjection,
            peritus_projection::RecoveryClass::RepairJournal => RecoveryClass::TerminalIntegrity,
            peritus_projection::RecoveryClass::CorrectInput => RecoveryClass::CorrectInput,
        };
        Self {
            kind: TraceErrorKind::Projection,
            recovery,
            operation,
            detail: "trace projection operation failed",
            source_class: Some("projection"),
        }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> TraceErrorKind {
        self.kind
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns the content-free operation name.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Debug for TraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TraceError")
            .field("kind", &self.kind)
            .field("recovery", &self.recovery)
            .field("operation", &self.operation)
            .field("detail", &self.detail)
            .field("source", &self.source_class)
            .finish()
    }
}

impl fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {}: {}", self.code(), self.operation, self.detail)
    }
}

impl Error for TraceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

const fn recovery(kind: TraceErrorKind) -> RecoveryClass {
    match kind {
        TraceErrorKind::InvalidIdentity
        | TraceErrorKind::InvalidBinding
        | TraceErrorKind::LimitExceeded
        | TraceErrorKind::NonCanonical
        | TraceErrorKind::Sequence
        | TraceErrorKind::InvalidTransition
        | TraceErrorKind::TimeRegression
        | TraceErrorKind::Redaction
        | TraceErrorKind::InvalidFrame => RecoveryClass::CorrectInput,
        TraceErrorKind::CausalIntegrity
        | TraceErrorKind::DuplicateConflict
        | TraceErrorKind::Integrity => RecoveryClass::TerminalIntegrity,
        TraceErrorKind::Storage => RecoveryClass::Retry,
        TraceErrorKind::Projection => RecoveryClass::RebuildProjection,
    }
}
