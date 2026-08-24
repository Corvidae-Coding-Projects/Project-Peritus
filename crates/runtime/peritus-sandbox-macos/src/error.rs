//! Stable macOS backend errors.

use core::fmt;

const MAX_DETAIL_BYTES: usize = 512;

/// Stable macOS backend failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MacosErrorKind {
    /// Input or canonical data is invalid.
    InvalidInput,
    /// A bounded collection or representation exceeded its ceiling.
    LimitExceeded,
    /// The current host cannot provide an exact required control.
    UnsupportedHost,
    /// Host capability probing failed.
    ProbeFailed,
    /// The admitted descriptor does not match this backend.
    DescriptorMismatch,
    /// Plan, manifest, or preparation identities disagree.
    PreparationMismatch,
    /// A path or Seatbelt rule cannot be represented exactly.
    ProfileCompilation,
    /// The native helper rejected or failed its protocol.
    HelperFailure,
    /// Seatbelt denied activation.
    SandboxDenied,
    /// A resource limit was crossed or could not be installed.
    ResourceLimit,
    /// The process-owned supervisor reported a failure.
    SupervisorFailure,
    /// An observation is missing, duplicated, or incorrectly bound.
    ObservationMismatch,
    /// Cleanup did not prove that every owned resource was released.
    CleanupIncomplete,
    /// Recovery could not classify an exact owned native identity.
    RecoveryIndeterminate,
    /// An operating-system I/O operation failed.
    Io,
}

impl MacosErrorKind {
    /// Returns the stable machine-readable code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-MACOS-INPUT-001",
            Self::LimitExceeded => "PERITUS-MACOS-LIMIT-001",
            Self::UnsupportedHost => "PERITUS-MACOS-UNSUPPORTED-001",
            Self::ProbeFailed => "PERITUS-MACOS-PROBE-001",
            Self::DescriptorMismatch => "PERITUS-MACOS-DESCRIPTOR-001",
            Self::PreparationMismatch => "PERITUS-MACOS-PREPARATION-001",
            Self::ProfileCompilation => "PERITUS-MACOS-PROFILE-001",
            Self::HelperFailure => "PERITUS-MACOS-HELPER-001",
            Self::SandboxDenied => "PERITUS-MACOS-SEATBELT-001",
            Self::ResourceLimit => "PERITUS-MACOS-RESOURCE-001",
            Self::SupervisorFailure => "PERITUS-MACOS-SUPERVISOR-001",
            Self::ObservationMismatch => "PERITUS-MACOS-OBSERVATION-001",
            Self::CleanupIncomplete => "PERITUS-MACOS-CLEANUP-001",
            Self::RecoveryIndeterminate => "PERITUS-MACOS-RECOVERY-001",
            Self::Io => "PERITUS-MACOS-IO-001",
        }
    }
}

/// Backend operation in progress at failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MacosOperation {
    /// Validate caller-owned input.
    Validate,
    /// Probe current host controls.
    Probe,
    /// Compile the Seatbelt profile.
    CompileProfile,
    /// Encode or decode a helper manifest.
    Manifest,
    /// Prepare inert native state.
    Prepare,
    /// Activate the helper and native controls.
    Activate,
    /// Request process-tree cancellation.
    Cancel,
    /// Observe target termination.
    Terminate,
    /// Release all backend-owned state.
    Release,
    /// Reopen and reconcile durable state.
    Recover,
}

/// Stable recovery guidance for a macOS failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryAction {
    /// Correct malformed or unrepresentable request data.
    CorrectRequest,
    /// Select a backend whose probe covers the checked requirements.
    SelectSupportedBackend,
    /// Re-probe and create a new authorization.
    Reauthorize,
    /// Install or repair the packaged helper.
    RepairHelper,
    /// Cancel and reap the complete process group.
    CancelAndReap,
    /// Retry cleanup for the exact recorded identity.
    RetryCleanup,
    /// Reopen records and reconcile native ownership.
    Reconcile,
    /// Quarantine the ambiguous record for operator action.
    Quarantine,
}

/// Typed backend error with bounded, non-sensitive detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacosError {
    kind: MacosErrorKind,
    operation: MacosOperation,
    recovery: RecoveryAction,
    detail: String,
}

impl MacosError {
    /// Constructs a bounded typed error.
    #[must_use]
    pub fn new(
        kind: MacosErrorKind,
        operation: MacosOperation,
        recovery: RecoveryAction,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        if detail.len() > MAX_DETAIL_BYTES {
            let boundary = detail
                .char_indices()
                .map(|(index, _)| index)
                .take_while(|index| *index <= MAX_DETAIL_BYTES)
                .last()
                .unwrap_or(0);
            detail.truncate(boundary);
        }
        Self { kind, operation, recovery, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> MacosErrorKind {
        self.kind
    }

    /// Returns the stable machine code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> MacosOperation {
        self.operation
    }

    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryAction {
        self.recovery
    }

    /// Returns bounded non-sensitive detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for MacosError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)
    }
}

impl std::error::Error for MacosError {}

pub(crate) fn invalid(operation: MacosOperation, detail: impl Into<String>) -> MacosError {
    MacosError::new(MacosErrorKind::InvalidInput, operation, RecoveryAction::CorrectRequest, detail)
}

pub(crate) fn limited(operation: MacosOperation, detail: impl Into<String>) -> MacosError {
    MacosError::new(
        MacosErrorKind::LimitExceeded,
        operation,
        RecoveryAction::CorrectRequest,
        detail,
    )
}

pub(crate) fn mismatch(kind: MacosErrorKind, detail: impl Into<String>) -> MacosError {
    MacosError::new(kind, MacosOperation::Prepare, RecoveryAction::Reauthorize, detail)
}

pub(crate) fn io_error(operation: MacosOperation, error: &std::io::Error) -> MacosError {
    let kind = error.kind();
    MacosError::new(
        MacosErrorKind::Io,
        operation,
        RecoveryAction::Reconcile,
        format!("operating-system I/O failure ({kind:?})"),
    )
}
