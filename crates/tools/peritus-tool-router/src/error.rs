//! Stable router failure vocabulary.

use core::fmt;

use crate::{ControlRetryability, DispatchFailure, InvocationHandle};

/// Stable router error category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouterErrorKind {
    /// Descriptor registry is malformed or disagrees with B1.
    Registry,
    /// Tool is unknown or not exposed.
    Exposure,
    /// Call preparation failed before authority.
    Preparation,
    /// Committed authority is absent, stale, or mismatched.
    Authorization,
    /// Dispatcher identity differs from the registered implementation.
    DispatcherIdentity,
    /// Router allocation configuration is invalid.
    Capacity,
    /// Durable replay history is unavailable or inconsistent.
    Durability,
    /// Action identity was reused with different bound bytes.
    ReplayConflict,
    /// A prior non-idempotent outcome must not be repeated.
    PriorOutcome,
    /// Active invocation or control is unknown/unsupported.
    Control,
    /// Dispatcher result/progress is malformed.
    InvalidObservation,
    /// Recovery cannot establish a safe outcome.
    Indeterminate,
}

/// Bounded typed router failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterError {
    kind: RouterErrorKind,
    operation: &'static str,
    detail: &'static str,
    retained_invocation: Option<InvocationHandle>,
    control_retryability: Option<ControlRetryability>,
    dispatch_failure: Option<Box<DispatchFailure>>,
}

impl RouterError {
    pub(crate) const fn new(
        kind: RouterErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self {
            kind,
            operation,
            detail,
            retained_invocation: None,
            control_retryability: None,
            dispatch_failure: None,
        }
    }

    pub(crate) fn retaining_invocation(mut self, handle: InvocationHandle) -> Self {
        self.retained_invocation = Some(handle);
        self
    }

    pub(crate) fn rejecting_control(mut self, retryability: ControlRetryability) -> Self {
        self.control_retryability = Some(retryability);
        self
    }

    pub(crate) fn active_failure(
        handle: InvocationHandle,
        operation: &'static str,
        failure: DispatchFailure,
        control_retryability: Option<ControlRetryability>,
    ) -> Self {
        let (kind, detail) = if control_retryability.is_some() {
            (
                RouterErrorKind::Control,
                "control was rejected before admission; invocation remains owned",
            )
        } else {
            (
                RouterErrorKind::Indeterminate,
                "operation failed; invocation remains owned for reconciliation",
            )
        };
        Self {
            kind,
            operation,
            detail,
            retained_invocation: Some(handle),
            control_retryability,
            dispatch_failure: Some(Box::new(failure)),
        }
    }

    /// Returns the exact invocation still owned after this operation failed.
    #[must_use]
    pub const fn retained_invocation(&self) -> Option<InvocationHandle> {
        self.retained_invocation
    }

    /// Returns retry guidance only for a control proven not to have been admitted.
    ///
    /// `None` means the caller must reconcile the invocation without resending an effectful
    /// control. This receipt never authorizes a second invocation dispatch.
    #[must_use]
    pub const fn control_retryability(&self) -> Option<ControlRetryability> {
        self.control_retryability
    }

    /// Borrows the original lower-boundary failure without terminal normalization.
    #[must_use]
    pub fn dispatch_failure(&self) -> Option<&DispatchFailure> {
        self.dispatch_failure.as_deref()
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> RouterErrorKind {
        self.kind
    }
    /// Returns the failed router operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Returns bounded stable detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for RouterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)?;
        if let Some(failure) = &self.dispatch_failure {
            write!(formatter, ": {failure}")?;
        }
        Ok(())
    }
}

impl std::error::Error for RouterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.dispatch_failure
            .as_deref()
            .map(|failure| failure as &(dyn std::error::Error + 'static))
    }
}

impl From<peritus_tool_protocol::ProtocolError> for RouterError {
    fn from(_: peritus_tool_protocol::ProtocolError) -> Self {
        Self::new(
            RouterErrorKind::Preparation,
            "prepare tool call",
            "tool call failed bounded protocol or schema validation",
        )
    }
}
