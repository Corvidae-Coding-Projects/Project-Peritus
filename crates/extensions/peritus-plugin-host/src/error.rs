//! Stable host failure classification and caller-owned diagnostic evidence.

use std::{error::Error, fmt};

const MAX_RENDERED_DETAIL_BYTES: usize = 1024;

/// Host-observed failure class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostFailureClass {
    /// Plugin discovery or manifest validation failed.
    Discovery,
    /// Artifact or manifest trust was not established.
    Trust,
    /// Current B1/G0 mediation denied the request.
    Authorization,
    /// A configured resource ceiling was exhausted.
    Quota,
    /// Plugin protocol or correlation was invalid.
    Protocol,
    /// Process/Wasm runtime could not be launched or communicated with.
    Infrastructure,
    /// Plugin process exited or reported its own failure.
    Plugin,
    /// Cooperative cancellation completed.
    Cancelled,
    /// Invocation deadline elapsed.
    Timeout,
    /// Effect completion could not be established.
    Indeterminate,
}

/// Safe next-step classification for callers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryDisposition {
    /// Correct configuration or input before retrying.
    CorrectRequest,
    /// Establish explicit trust before retrying.
    EstablishTrust,
    /// Obtain fresh authority for a new action.
    Reauthorize,
    /// Wait for capacity before a new request.
    RetryLater,
    /// Restart the isolated plugin before a new request.
    RestartPlugin,
    /// Reconcile the possibly completed external effect first.
    Reconcile,
    /// No recovery action is required.
    None,
}

/// Complete in-memory failure evidence whose disclosure and persistence remain caller-owned.
///
/// This value is never written to [`crate::HostStateStore`]. Its detail and optional cause may
/// contain information supplied by an authority mediator or operating system, so callers must
/// apply their own disclosure and credential-redaction policy before exporting it.
pub struct HostDiagnosticEvidence {
    class: HostFailureClass,
    recovery: RecoveryDisposition,
    operation: &'static str,
    detail: String,
    cause: Option<Box<dyn Error + Send + Sync>>,
}

impl HostDiagnosticEvidence {
    /// Returns the original stable failure class.
    #[must_use]
    pub const fn class(&self) -> HostFailureClass {
        self.class
    }

    /// Returns the original safe recovery disposition.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryDisposition {
        self.recovery
    }

    /// Returns the original failing operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows the complete original diagnostic detail without rendering truncation.
    ///
    /// Callers choose whether and where this value may be disclosed or persisted.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Borrows the complete original cause, when one was supplied.
    #[must_use]
    pub fn cause(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
        self.cause.as_deref()
    }

    /// Borrows the character-safe bounded detail used by routine rendering.
    #[must_use]
    pub fn rendered_detail(&self) -> &str {
        utf8_prefix(&self.detail, MAX_RENDERED_DETAIL_BYTES)
    }
}

impl fmt::Debug for HostDiagnosticEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostDiagnosticEvidence")
            .field("class", &self.class)
            .field("recovery", &self.recovery)
            .field("operation", &self.operation)
            .field("detail", &self.rendered_detail())
            .field("cause", &self.cause.as_ref().map(|_| "[retained]"))
            .finish()
    }
}

/// Typed host error with bounded rendering and complete caller-accessible evidence.
pub struct HostError {
    evidence: HostDiagnosticEvidence,
}

impl HostError {
    /// Creates an error without an underlying cause.
    #[must_use]
    pub fn new(
        class: HostFailureClass,
        recovery: RecoveryDisposition,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::build(class, recovery, operation, detail.into(), None)
    }

    /// Creates an error preserving the complete underlying cause.
    pub fn with_source(
        class: HostFailureClass,
        recovery: RecoveryDisposition,
        operation: &'static str,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::build(class, recovery, operation, detail.into(), Some(Box::new(source)))
    }

    fn build(
        class: HostFailureClass,
        recovery: RecoveryDisposition,
        operation: &'static str,
        detail: String,
        cause: Option<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        Self {
            evidence: HostDiagnosticEvidence { class, recovery, operation, detail, cause },
        }
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn class(&self) -> HostFailureClass {
        self.evidence.class()
    }

    /// Returns the safe recovery disposition.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryDisposition {
        self.evidence.recovery()
    }

    /// Returns the failing operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.evidence.operation()
    }

    /// Borrows character-safe diagnostic detail bounded for routine rendering.
    #[must_use]
    pub fn detail(&self) -> &str {
        self.evidence.rendered_detail()
    }

    /// Borrows the complete diagnostic evidence for an explicit caller policy decision.
    #[must_use]
    pub const fn evidence(&self) -> &HostDiagnosticEvidence {
        &self.evidence
    }

    /// Transfers the complete diagnostic evidence to the caller.
    #[must_use]
    pub fn into_evidence(self) -> HostDiagnosticEvidence {
        self.evidence
    }
}

impl fmt::Debug for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("HostError").field(&self.evidence).finish()
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation(), self.detail())
    }
}

impl Error for HostError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.evidence
            .cause()
            .map(|cause| cause as &(dyn Error + 'static))
    }
}

fn utf8_prefix(value: &str, maximum: usize) -> &str {
    if value.len() <= maximum {
        return value;
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    &value[..end]
}
