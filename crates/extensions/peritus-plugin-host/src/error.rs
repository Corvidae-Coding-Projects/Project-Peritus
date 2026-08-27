//! Stable host failure classification.

use std::{error::Error, fmt};

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

/// Typed host error with bounded diagnostic detail.
#[derive(Debug)]
pub struct HostError {
    class: HostFailureClass,
    recovery: RecoveryDisposition,
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl HostError {
    /// Creates an error without an underlying source.
    #[must_use]
    pub fn new(
        class: HostFailureClass,
        recovery: RecoveryDisposition,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::build(class, recovery, operation, detail.into(), None)
    }

    /// Creates an error preserving an underlying source.
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
        mut detail: String,
        source: Option<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        detail.truncate(1024);
        Self { class, recovery, operation, detail, source }
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn class(&self) -> HostFailureClass {
        self.class
    }

    /// Returns the safe recovery disposition.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryDisposition {
        self.recovery
    }

    /// Returns the failing operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows bounded causal detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl Error for HostError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}
