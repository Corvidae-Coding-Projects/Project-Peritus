//! Stable sandbox failures and recovery guidance.

use crate::FeatureSet;

/// Stable sandbox failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SandboxErrorKind {
    /// A domain value or collection is invalid.
    InvalidInput,
    /// A bounded input or observation ceiling was exceeded.
    LimitExceeded,
    /// Requirements are not admitted by the declared contract.
    RequirementDenied,
    /// A backend cannot enforce every required feature.
    UnsupportedBackend,
    /// A backend or preparation identity differs from the checked plan.
    BackendMismatch,
    /// A sandbox lifecycle transition is illegal.
    IllegalTransition,
    /// Reference resource accounting crossed a declared ceiling.
    ResourceLimit,
    /// A named deterministic reference fault fired.
    InjectedFault,
    /// A cancelled operation cannot proceed.
    Cancelled,
}

impl SandboxErrorKind {
    /// Returns the stable machine-readable error code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-SANDBOX-INPUT-001",
            Self::LimitExceeded => "PERITUS-SANDBOX-LIMIT-001",
            Self::RequirementDenied => "PERITUS-SANDBOX-POLICY-001",
            Self::UnsupportedBackend => "PERITUS-SANDBOX-BACKEND-001",
            Self::BackendMismatch => "PERITUS-SANDBOX-BACKEND-002",
            Self::IllegalTransition => "PERITUS-SANDBOX-LIFECYCLE-001",
            Self::ResourceLimit => "PERITUS-SANDBOX-RESOURCE-001",
            Self::InjectedFault => "PERITUS-SANDBOX-REFERENCE-001",
            Self::Cancelled => "PERITUS-SANDBOX-CANCEL-001",
        }
    }
}

/// Operation in progress when a sandbox failure occurred.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SandboxOperation {
    /// Validate a domain value.
    Validate,
    /// Compile a checked plan.
    Compile,
    /// Admit a backend.
    AdmitBackend,
    /// Prepare a backend session.
    Prepare,
    /// Activate a prepared session.
    Activate,
    /// Evaluate a reference probe.
    Evaluate,
    /// Account resource usage.
    Account,
    /// Request cancellation.
    Cancel,
    /// Observe termination.
    Terminate,
    /// Release backend state.
    Release,
}

/// Stable recovery classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct malformed or denied request data.
    CorrectRequest,
    /// Select or install a backend with complete enforcement.
    SelectBackend,
    /// Rebuild the plan against current backend facts.
    Replan,
    /// Retry a non-authoritative reference operation.
    Retry,
    /// Cancel and release the current sandbox session.
    CancelAndRelease,
    /// Reconcile an incomplete lifecycle observation.
    Reconcile,
    /// The request is terminal and must use a new identity.
    Terminal,
}

/// Typed sandbox failure with bounded static detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxError {
    kind: SandboxErrorKind,
    operation: SandboxOperation,
    recovery: RecoveryClass,
    detail: &'static str,
    missing_features: FeatureSet,
}

impl SandboxError {
    /// Creates a failure without a backend feature gap.
    #[must_use]
    pub const fn new(
        kind: SandboxErrorKind,
        operation: SandboxOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail, missing_features: FeatureSet::empty() }
    }

    /// Creates an unsupported-backend failure with the exact missing feature set.
    #[must_use]
    pub const fn unsupported(missing_features: FeatureSet, detail: &'static str) -> Self {
        Self {
            kind: SandboxErrorKind::UnsupportedBackend,
            operation: SandboxOperation::AdmitBackend,
            recovery: RecoveryClass::SelectBackend,
            detail,
            missing_features,
        }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> SandboxErrorKind {
        self.kind
    }
    /// Returns the stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }
    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> SandboxOperation {
        self.operation
    }
    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
    /// Returns bounded non-sensitive detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
    /// Returns the exact missing backend features, if any.
    #[must_use]
    pub const fn missing_features(&self) -> FeatureSet {
        self.missing_features
    }
}

impl core::fmt::Display for SandboxError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)
    }
}

impl std::error::Error for SandboxError {}

/// Builds an internal invalid-input error.
pub const fn invalid(detail: &'static str) -> SandboxError {
    SandboxError::new(
        SandboxErrorKind::InvalidInput,
        SandboxOperation::Validate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

/// Builds an internal input-bound error.
pub const fn bound(detail: &'static str) -> SandboxError {
    SandboxError::new(
        SandboxErrorKind::LimitExceeded,
        SandboxOperation::Validate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

/// Builds an internal contract-denial error.
pub const fn denied(detail: &'static str) -> SandboxError {
    SandboxError::new(
        SandboxErrorKind::RequirementDenied,
        SandboxOperation::Compile,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

/// Builds an internal reference-fault error.
pub const fn injected(operation: SandboxOperation) -> SandboxError {
    SandboxError::new(
        SandboxErrorKind::InjectedFault,
        operation,
        RecoveryClass::Retry,
        "deterministic reference fault injected",
    )
}
