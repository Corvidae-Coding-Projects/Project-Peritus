//! Stable Windows backend failures and recovery guidance.

use core::fmt;

use peritus_network::{
    NetworkError, NetworkErrorKind, NetworkOperation, RecoveryClass as NetworkRecovery,
};
use peritus_process::{
    ErrorCode as ProcessErrorCode, ProcessError, ProcessOperation,
    RecoveryClass as ProcessRecovery,
};
use peritus_secrets::{
    RecoveryClass as SecretRecovery, SecretError, SecretErrorKind, SecretOperation,
};

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

/// Stable typed cause retained beneath one Windows backend failure.
///
/// These snapshots contain only machine-readable categories and never paths, resource names,
/// secret material, or provider diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsErrorSource {
    /// Operating-system I/O category.
    Io(std::io::ErrorKind),
    /// Managed-network preparation or teardown failure.
    Network {
        /// Underlying network category.
        kind: NetworkErrorKind,
        /// Underlying network operation.
        operation: NetworkOperation,
        /// Underlying network recovery requirement.
        recovery: NetworkRecovery,
    },
    /// Secret lookup, delivery, or cleanup failure.
    Secret {
        /// Underlying secret category.
        kind: SecretErrorKind,
        /// Underlying secret operation.
        operation: SecretOperation,
        /// Underlying secret recovery requirement.
        recovery: SecretRecovery,
    },
    /// Protected-handle or process integration failure.
    Process {
        /// Underlying process category.
        code: ProcessErrorCode,
        /// Underlying process operation.
        operation: ProcessOperation,
        /// Underlying process recovery requirement.
        recovery: ProcessRecovery,
    },
}

impl fmt::Display for WindowsErrorSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(kind) => write!(formatter, "operating-system I/O category {kind:?}"),
            Self::Network { kind, operation, recovery } => {
                write!(formatter, "network {kind:?} during {operation:?}; recovery {recovery:?}")
            }
            Self::Secret { kind, operation, recovery } => {
                write!(formatter, "secret {kind:?} during {operation:?}; recovery {recovery:?}")
            }
            Self::Process { code, operation, recovery } => {
                write!(formatter, "process {code:?} during {operation:?}; recovery {recovery:?}")
            }
        }
    }
}

impl std::error::Error for WindowsErrorSource {}

/// Exact resource families whose cleanup could not be proven after preparation failed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreparationCleanup {
    acl_restore: bool,
    filter_release: bool,
    proxy_shutdown: bool,
    secret_release: bool,
}

impl PreparationCleanup {
    pub(crate) const fn new(
        acl_restore: bool,
        filter_release: bool,
        proxy_shutdown: bool,
        secret_release: bool,
    ) -> Self {
        Self { acl_restore, filter_release, proxy_shutdown, secret_release }
    }

    pub(crate) const fn merge(self, other: Self) -> Self {
        Self {
            acl_restore: self.acl_restore || other.acl_restore,
            filter_release: self.filter_release || other.filter_release,
            proxy_shutdown: self.proxy_shutdown || other.proxy_shutdown,
            secret_release: self.secret_release || other.secret_release,
        }
    }

    /// Reports whether temporary ACL restoration still requires reconciliation.
    #[must_use]
    pub const fn acl_restore(self) -> bool {
        self.acl_restore
    }

    /// Reports whether dynamic WFP policy release still requires reconciliation.
    #[must_use]
    pub const fn filter_release(self) -> bool {
        self.filter_release
    }

    /// Reports whether managed-proxy shutdown still requires reconciliation.
    #[must_use]
    pub const fn proxy_shutdown(self) -> bool {
        self.proxy_shutdown
    }

    /// Reports whether exact secret delivery cleanup still requires reconciliation.
    #[must_use]
    pub const fn secret_release(self) -> bool {
        self.secret_release
    }

    /// Reports whether preparation proved every started resource family clean.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        !self.acl_restore && !self.filter_release && !self.proxy_shutdown && !self.secret_release
    }
}

/// Typed Windows backend error with bounded nonsensitive detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsError {
    kind: WindowsErrorKind,
    operation: WindowsOperation,
    recovery: WindowsRecovery,
    detail: String,
    source: Option<WindowsErrorSource>,
    cleanup: PreparationCleanup,
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
        Self {
            kind,
            operation,
            recovery,
            detail,
            source: None,
            cleanup: PreparationCleanup::new(false, false, false, false),
        }
    }

    pub(crate) fn with_source(mut self, source: WindowsErrorSource) -> Self {
        self.source = Some(source);
        self
    }

    pub(crate) fn with_cleanup(mut self, cleanup: PreparationCleanup) -> Self {
        self.cleanup = self.cleanup.merge(cleanup);
        self
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

    /// Returns the stable typed underlying failure, when one was available.
    #[must_use]
    pub const fn cause(&self) -> Option<WindowsErrorSource> {
        self.source
    }

    /// Returns exact incomplete resource-family cleanup after preparation failure.
    #[must_use]
    pub const fn preparation_cleanup(&self) -> PreparationCleanup {
        self.cleanup
    }
}

impl fmt::Display for WindowsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}/{:?}: {}", self.kind, self.operation, self.detail)
    }
}

impl std::error::Error for WindowsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub(crate) const fn network_source(error: &NetworkError) -> WindowsErrorSource {
    WindowsErrorSource::Network {
        kind: error.kind(),
        operation: error.operation(),
        recovery: error.recovery(),
    }
}

pub(crate) const fn secret_source(error: &SecretError) -> WindowsErrorSource {
    WindowsErrorSource::Secret {
        kind: error.kind(),
        operation: error.operation(),
        recovery: error.recovery(),
    }
}

pub(crate) const fn process_source(error: &ProcessError) -> WindowsErrorSource {
    WindowsErrorSource::Process {
        code: error.code(),
        operation: error.operation(),
        recovery: error.recovery(),
    }
}

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
