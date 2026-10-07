//! Stable process failures and recovery guidance.

use core::fmt;

/// Stable machine-readable process failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCode {
    /// Structured input is outside its checked bound.
    InvalidInput,
    /// The working directory cannot be established exactly.
    InvalidWorkingDirectory,
    /// The deterministic environment is invalid.
    InvalidEnvironment,
    /// A sandbox or execution-plan binding differs.
    PlanMismatch,
    /// A required backend or platform feature is absent.
    Unsupported,
    /// B0, B1, B3, or C0 authority differs.
    AuthorizationMismatch,
    /// The exact committed dispatch event is absent.
    MissingDispatch,
    /// Budget authority is absent, stale, or inadequate.
    BudgetMismatch,
    /// Required lease authority is absent, stale, or surplus.
    LeaseMismatch,
    /// The action/process authorization was already consumed.
    ReceiptReused,
    /// Durable intent or lifecycle persistence failed.
    Persistence,
    /// Operating-system process creation failed.
    Spawn,
    /// PTY creation or control failed.
    Pty,
    /// Process input failed.
    Input,
    /// Process output or spool handling failed.
    Output,
    /// A process-tree operation failed.
    ProcessTree,
    /// A configured resource limit terminated execution.
    ResourceLimit,
    /// Artifact publication failed.
    Artifact,
    /// A recovery record is malformed or inconsistent.
    CorruptRecovery,
    /// Recovery cannot establish an exact safe observation.
    Indeterminate,
    /// A support task failed or panicked.
    Supervisor,
}

impl ErrorCode {
    /// Returns the stable external code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-PROCESS-001",
            Self::InvalidWorkingDirectory => "PERITUS-PROCESS-002",
            Self::InvalidEnvironment => "PERITUS-PROCESS-003",
            Self::PlanMismatch => "PERITUS-PROCESS-004",
            Self::Unsupported => "PERITUS-PROCESS-005",
            Self::AuthorizationMismatch => "PERITUS-PROCESS-006",
            Self::MissingDispatch => "PERITUS-PROCESS-007",
            Self::BudgetMismatch => "PERITUS-PROCESS-008",
            Self::LeaseMismatch => "PERITUS-PROCESS-009",
            Self::ReceiptReused => "PERITUS-PROCESS-010",
            Self::Persistence => "PERITUS-PROCESS-011",
            Self::Spawn => "PERITUS-PROCESS-012",
            Self::Pty => "PERITUS-PROCESS-013",
            Self::Input => "PERITUS-PROCESS-014",
            Self::Output => "PERITUS-PROCESS-015",
            Self::ProcessTree => "PERITUS-PROCESS-016",
            Self::ResourceLimit => "PERITUS-PROCESS-017",
            Self::Artifact => "PERITUS-PROCESS-018",
            Self::CorruptRecovery => "PERITUS-PROCESS-019",
            Self::Indeterminate => "PERITUS-PROCESS-020",
            Self::Supervisor => "PERITUS-PROCESS-021",
        }
    }
}

/// Operation in progress when a process failure was observed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProcessOperation {
    /// Validate a structured request.
    Validate,
    /// Open the protected process registry.
    OpenStore,
    /// Validate committed authority.
    Authorize,
    /// Persist execution state.
    Persist,
    /// Create the operating-system process.
    Spawn,
    /// Exchange process input or output.
    Stream,
    /// Control or terminate the owned process tree.
    Control,
    /// Wait for owned work and publish a terminal result.
    Wait,
    /// Publish retained bytes as an artifact.
    PublishArtifact,
    /// Reopen and reconcile durable process state.
    Reconcile,
    /// Inspect exact lease-holder quiescence.
    InspectQuiescence,
}

/// Stable caller recovery category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct the request before trying again.
    CorrectRequest,
    /// Obtain fresh committed authority.
    Reauthorize,
    /// Select a backend that covers the complete plan.
    SelectBackend,
    /// Retry preparation before an effect is consumed.
    RetryPreparation,
    /// Cancel and reap the owned process tree.
    CancelAndReap,
    /// Reopen the durable registry and reconcile.
    ReopenAndReconcile,
    /// Isolate the record for operator inspection.
    Quarantine,
    /// Retry artifact publication from the retained spool.
    RetryPublication,
    /// The observation is terminal and needs no retry.
    Terminal,
}

/// Reason a control was rejected before entering the process owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRejection {
    /// No control was accepted; the same request may be retried when capacity becomes available.
    Backpressure,
    /// No control was accepted; the request must be corrected before retry.
    InvalidRequest,
    /// No control was accepted; observe the original invocation instead of resending this control.
    AdmissionClosed,
}

/// Bounded typed process failure.
#[derive(Debug)]
pub struct ProcessError {
    code: ErrorCode,
    operation: ProcessOperation,
    recovery: RecoveryClass,
    detail: &'static str,
    control_rejection: Option<ControlRejection>,
    preparation_cleanup_complete: Option<bool>,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl ProcessError {
    /// Creates a bounded non-content-bearing failure.
    #[must_use]
    pub const fn new(
        code: ErrorCode,
        operation: ProcessOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self {
            code,
            operation,
            recovery,
            detail,
            control_rejection: None,
            preparation_cleanup_complete: None,
            source: None,
        }
    }

    /// Creates a bounded failure while retaining one typed, non-content-bearing local cause.
    ///
    /// Source text is never serialized across the retained-owner protocol. Callers must keep the
    /// public detail static and safe independently of the supplied source.
    #[must_use]
    pub fn with_source(
        code: ErrorCode,
        operation: ProcessOperation,
        recovery: RecoveryClass,
        detail: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            code,
            operation,
            recovery,
            detail,
            control_rejection: None,
            preparation_cleanup_complete: None,
            source: Some(Box::new(source)),
        }
    }

    /// Records whether a failed authorized preparation proved every started resource released.
    ///
    /// This is local lifecycle evidence, not permission to redispatch a consumed authorization.
    #[must_use]
    pub const fn with_preparation_cleanup(mut self, complete: bool) -> Self {
        self.preparation_cleanup_complete = Some(complete);
        self
    }

    pub(crate) const fn rejecting_control(mut self, rejection: ControlRejection) -> Self {
        self.control_rejection = Some(rejection);
        self
    }

    /// Reconstructs a bounded error received from the authenticated retained service owner.
    ///
    /// The transport carries stable enums only. Local source chains and dynamic text never cross
    /// the owner boundary, so this uses one fixed diagnostic while preserving retry semantics.
    #[must_use]
    pub const fn from_retained_owner(
        code: ErrorCode,
        operation: ProcessOperation,
        recovery: RecoveryClass,
        control_rejection: Option<ControlRejection>,
    ) -> Self {
        Self {
            code,
            operation,
            recovery,
            detail: "retained process owner reported a failure",
            control_rejection,
            preparation_cleanup_complete: None,
            source: None,
        }
    }

    /// Returns retry guidance only when the control failed before admission.
    ///
    /// An untagged error does not establish that an effectful control is safe to repeat.
    #[must_use]
    pub const fn control_rejection(&self) -> Option<ControlRejection> {
        self.control_rejection
    }

    /// Returns exact cleanup evidence for a failed authorized preparation, when supplied.
    #[must_use]
    pub const fn preparation_cleanup_complete(&self) -> Option<bool> {
        self.preparation_cleanup_complete
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> ProcessOperation {
        self.operation
    }

    /// Returns the required recovery family.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns bounded safe context.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }

    /// Returns the exact typed local cause when one was available.
    #[must_use]
    pub fn cause(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.code.as_str(), self.operation, self.detail)
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub(crate) const fn invalid(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::Validate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

pub(crate) const fn mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::AuthorizationMismatch,
        ProcessOperation::Authorize,
        RecoveryClass::Reauthorize,
        detail,
    )
}
