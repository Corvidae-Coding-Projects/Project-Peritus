//! Stable Windows backend failures and recovery guidance.

use core::fmt;

const MAX_DETAIL_BYTES: usize = 512;

/// Stable Windows backend failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowsErrorKind {
    /// A checked value cannot be represented by this backend.
    InvalidPlan,
    /// Required operating-system support is unavailable.
    UnsupportedHost,
    /// A bounded capability probe failed or contradicted itself.
    ProbeFailed,
    /// Descriptor identity differs from the probed implementation.
    DescriptorMismatch,
    /// Preparation identity differs from C2 admission.
    PreparationMismatch,
    /// A helper protocol frame or handshake is invalid.
    HelperProtocol,
    /// Native isolation denied activation.
    SandboxDenied,
    /// Windows path projection is invalid or ambiguous.
    Path,
    /// Temporary ACL installation or reversal failed.
    Acl,
    /// Restricted-token creation or use failed.
    Token,
    /// `AppContainer` identity or activation failed.
    AppContainer,
    /// Job Object installation, accounting, or teardown failed.
    Job,
    /// An inherited handle was missing, duplicated, or broader than declared.
    Handle,
    /// A terminal/ConPTY requirement is unavailable.
    Terminal,
    /// A resource ceiling cannot be enforced as declared.
    Resource,
    /// Managed network isolation is unavailable or mismatched.
    Network,
    /// A protected secret delivery failed.
    Secret,
    /// An observation is missing, duplicated, or out of order.
    Observation,
    /// Exact native ownership cannot be established during recovery.
    RecoveryIndeterminate,
    /// A bounded operating-system I/O operation failed.
    Io,
}

/// Stable Windows backend operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowsOperation {
    /// Validate configuration or checked values.
    Validate,
    /// Probe host facilities.
    Probe,
    /// Normalize and resolve paths.
    ResolvePath,
    /// Compile filesystem and ACL policy.
    CompileAcl,
    /// Install temporary ACLs.
    InstallAcl,
    /// Restore temporary ACLs.
    RestoreAcl,
    /// Encode or decode a helper manifest.
    Manifest,
    /// Prepare a native session.
    Prepare,
    /// Activate token, job, handles, and target.
    Activate,
    /// Record cancellation.
    Cancel,
    /// Observe termination.
    Terminate,
    /// Release native resources.
    Release,
    /// Reopen and classify native state.
    Recover,
}

/// Stable recovery route for one Windows failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowsRecovery {
    /// Correct an invalid request or installation path.
    CorrectRequest,
    /// Select a backend whose probe covers the plan.
    SelectBackend,
    /// Repair or reinstall the reviewed helper.
    RepairHelper,
    /// Configure the required Windows service or policy.
    ConfigureHost,
    /// Repeat authorization after plan or installation drift.
    Reauthorize,
    /// Recompute admission and native preparation.
    Replan,
    /// Terminate the owned tree and reap it.
    CancelAndReap,
    /// Retry exact resource cleanup.
    RetryCleanup,
    /// Quarantine ambiguous recovery state.
    Quarantine,
}

/// Typed Windows backend error with bounded nonsensitive detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsError {
    kind: WindowsErrorKind,
    operation: WindowsOperation,
    recovery: WindowsRecovery,
    detail: String,
}

impl WindowsError {
    /// Creates a stable typed error, bounding detail to 512 UTF-8 bytes.
    #[must_use]
    pub fn new(
        kind: WindowsErrorKind,
        operation: WindowsOperation,
        recovery: WindowsRecovery,
        detail: impl Into<String>,
    ) -> Self {
        let detail = bounded_detail(detail.into());
        Self { kind, operation, recovery, detail }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> WindowsErrorKind {
        self.kind
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> WindowsOperation {
        self.operation
    }

    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> WindowsRecovery {
        self.recovery
    }

    /// Returns bounded nonsensitive detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for WindowsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}/{:?}: {}", self.kind, self.operation, self.detail)
    }
}

impl std::error::Error for WindowsError {}

pub(crate) fn invalid(operation: WindowsOperation, detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::InvalidPlan,
        operation,
        WindowsRecovery::CorrectRequest,
        detail,
    )
}

pub(crate) fn mismatch(kind: WindowsErrorKind, detail: &'static str) -> WindowsError {
    WindowsError::new(kind, WindowsOperation::Prepare, WindowsRecovery::Replan, detail)
}

pub(crate) fn unsupported(operation: WindowsOperation, detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::UnsupportedHost,
        operation,
        WindowsRecovery::ConfigureHost,
        detail,
    )
}

pub(crate) fn io(operation: WindowsOperation, detail: &'static str) -> WindowsError {
    WindowsError::new(WindowsErrorKind::Io, operation, WindowsRecovery::RetryCleanup, detail)
}

fn bounded_detail(mut detail: String) -> String {
    if detail.len() <= MAX_DETAIL_BYTES {
        return detail;
    }
    let mut end = MAX_DETAIL_BYTES;
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail.truncate(end);
    detail
}
