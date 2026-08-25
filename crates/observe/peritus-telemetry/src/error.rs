//! Stable content-free telemetry failure vocabulary.

use core::fmt;
use std::error::Error;

/// Stable telemetry failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TelemetryErrorKind {
    /// Queue or checkpoint configuration is invalid.
    InvalidConfiguration,
    /// A checked counter or sequence overflowed.
    SequenceOverflow,
    /// Exporter failed explicitly.
    ExportFailed,
    /// Export acknowledgement did not match the pending batch.
    AckMismatch,
    /// Durable checkpoint bytes or identity are invalid.
    InvalidCheckpoint,
    /// Filesystem checkpoint storage failed.
    Storage,
    /// Restart recovery inputs do not share the same history prefix.
    RecoveryMismatch,
}

impl TelemetryErrorKind {
    /// Returns a compatibility-stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "PERITUS-TELEMETRY-CONFIG-001",
            Self::SequenceOverflow => "PERITUS-TELEMETRY-SEQUENCE-001",
            Self::ExportFailed => "PERITUS-TELEMETRY-EXPORT-001",
            Self::AckMismatch => "PERITUS-TELEMETRY-ACK-001",
            Self::InvalidCheckpoint => "PERITUS-TELEMETRY-CHECKPOINT-001",
            Self::Storage => "PERITUS-TELEMETRY-STORAGE-001",
            Self::RecoveryMismatch => "PERITUS-TELEMETRY-RECOVERY-001",
        }
    }
}

/// Stable recovery guidance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct caller configuration or a mismatched acknowledgement.
    CorrectInput,
    /// Retry the same exporter batch after bounded backoff.
    RetryExporter,
    /// Exporter rejected the batch terminally; retain it for operator disposition.
    TerminalExporter,
    /// Rebuild the buffer from C0 trace history and its last valid checkpoint.
    Rebuild,
    /// Checkpoint storage requires operator repair.
    RepairStorage,
}

/// Typed telemetry error whose formatting carries no observation, path, or exporter detail.
pub struct TelemetryError {
    kind: TelemetryErrorKind,
    recovery: RecoveryClass,
    operation: &'static str,
    detail: &'static str,
    source_class: Option<&'static str>,
    exporter_code: Option<crate::ExporterErrorCode>,
    exporter_retryable: Option<bool>,
}

impl TelemetryError {
    pub(crate) const fn new(
        kind: TelemetryErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self {
            kind,
            recovery: recovery(kind),
            operation,
            detail,
            source_class: None,
            exporter_code: None,
            exporter_retryable: None,
        }
    }

    pub(crate) const fn sourced(
        kind: TelemetryErrorKind,
        operation: &'static str,
        detail: &'static str,
        source_class: &'static str,
    ) -> Self {
        Self {
            kind,
            recovery: recovery(kind),
            operation,
            detail,
            source_class: Some(source_class),
            exporter_code: None,
            exporter_retryable: None,
        }
    }

    pub(crate) const fn exporter(operation: &'static str, error: crate::ExporterError) -> Self {
        Self {
            kind: TelemetryErrorKind::ExportFailed,
            recovery: if error.retryable() {
                RecoveryClass::RetryExporter
            } else {
                RecoveryClass::TerminalExporter
            },
            operation,
            detail: "telemetry exporter operation failed",
            source_class: Some("exporter"),
            exporter_code: Some(error.code()),
            exporter_retryable: Some(error.retryable()),
        }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> TelemetryErrorKind {
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
    /// Returns the static content-free operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Returns the adapter's stable failure class when export failed.
    #[must_use]
    pub const fn exporter_code(&self) -> Option<crate::ExporterErrorCode> {
        self.exporter_code
    }
    /// Returns the adapter's retry declaration when export failed.
    #[must_use]
    pub const fn exporter_retryable(&self) -> Option<bool> {
        self.exporter_retryable
    }
}

impl fmt::Debug for TelemetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TelemetryError")
            .field("kind", &self.kind)
            .field("recovery", &self.recovery)
            .field("operation", &self.operation)
            .field("detail", &self.detail)
            .field("source", &self.source_class)
            .field("exporter_code", &self.exporter_code)
            .field("exporter_retryable", &self.exporter_retryable)
            .finish()
    }
}

impl fmt::Display for TelemetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {}: {}", self.code(), self.operation, self.detail)
    }
}

impl Error for TelemetryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

const fn recovery(kind: TelemetryErrorKind) -> RecoveryClass {
    match kind {
        TelemetryErrorKind::InvalidConfiguration | TelemetryErrorKind::AckMismatch => {
            RecoveryClass::CorrectInput
        }
        TelemetryErrorKind::SequenceOverflow | TelemetryErrorKind::RecoveryMismatch => {
            RecoveryClass::Rebuild
        }
        TelemetryErrorKind::ExportFailed => RecoveryClass::RetryExporter,
        TelemetryErrorKind::InvalidCheckpoint | TelemetryErrorKind::Storage => {
            RecoveryClass::RepairStorage
        }
    }
}
