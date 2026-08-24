//! Stable Linux backend failures and recovery guidance.

const MAX_DETAIL_BYTES: usize = 512;

/// Stable Linux backend failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LinuxErrorKind {
    /// Input or checked-plan projection is invalid.
    InvalidPlan,
    /// The host cannot enforce a required facility.
    UnsupportedHost,
    /// A runtime capability probe failed.
    ProbeFailed,
    /// The selected descriptor is not this probed implementation.
    DescriptorMismatch,
    /// Plan, support, or preparation binding differs.
    PreparationMismatch,
    /// A canonical path or mount cannot be represented exactly.
    Filesystem,
    /// Native helper input or activation failed.
    Helper,
    /// Kernel policy installation denied activation.
    SandboxDenied,
    /// Cgroup creation, attachment, accounting, or removal failed.
    Cgroup,
    /// Resource policy cannot be installed faithfully.
    Resource,
    /// Managed network isolation or proxy routing failed.
    Network,
    /// Observation sequence or binding is invalid.
    Observation,
    /// Exact native ownership cannot be established during recovery.
    RecoveryIndeterminate,
    /// An operating-system I/O operation failed.
    Io,
}

impl LinuxErrorKind {
    /// Returns the stable subsystem code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidPlan => "PERITUS-LINUX-PLAN-001",
            Self::UnsupportedHost => "PERITUS-LINUX-HOST-001",
            Self::ProbeFailed => "PERITUS-LINUX-PROBE-001",
            Self::DescriptorMismatch => "PERITUS-LINUX-BACKEND-001",
            Self::PreparationMismatch => "PERITUS-LINUX-PREPARE-001",
            Self::Filesystem => "PERITUS-LINUX-FILESYSTEM-001",
            Self::Helper => "PERITUS-LINUX-HELPER-001",
            Self::SandboxDenied => "PERITUS-LINUX-DENIED-001",
            Self::Cgroup => "PERITUS-LINUX-CGROUP-001",
            Self::Resource => "PERITUS-LINUX-RESOURCE-001",
            Self::Network => "PERITUS-LINUX-NETWORK-001",
            Self::Observation => "PERITUS-LINUX-OBSERVATION-001",
            Self::RecoveryIndeterminate => "PERITUS-LINUX-RECOVERY-001",
            Self::Io => "PERITUS-LINUX-IO-001",
        }
    }
}

/// Operation in progress when a failure occurred.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LinuxOperation {
    /// Probe host facilities.
    Probe,
    /// Project a checked plan.
    Project,
    /// Validate a preparation binding.
    Prepare,
    /// Encode or decode helper protocol data.
    Manifest,
    /// Install runtime resources.
    Install,
    /// Activate native enforcement.
    Activate,
    /// Attach a process to its cgroup.
    Attach,
    /// Observe lifecycle state.
    Observe,
    /// Cancel the owned process tree.
    Cancel,
    /// Release native state.
    Release,
    /// Reopen a recovery record.
    Recover,
}

/// Stable recovery route.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LinuxRecovery {
    /// Correct the checked request or configuration.
    CorrectRequest,
    /// Install or enable the named Linux facility.
    ConfigureHost,
    /// Re-probe and obtain a new admission.
    Replan,
    /// Cancel and reap the owned process tree.
    CancelAndReap,
    /// Retry exact idempotent cleanup.
    RetryCleanup,
    /// Reopen state and reconcile exact ownership.
    Reconcile,
    /// Quarantine state because ownership cannot be proved.
    Quarantine,
}

/// Typed bounded Linux backend error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxError {
    kind: LinuxErrorKind,
    operation: LinuxOperation,
    recovery: LinuxRecovery,
    detail: String,
}

impl LinuxError {
    /// Creates an error while bounding non-sensitive detail.
    #[must_use]
    pub fn new(
        kind: LinuxErrorKind,
        operation: LinuxOperation,
        recovery: LinuxRecovery,
        detail: impl AsRef<str>,
    ) -> Self {
        let detail = detail.as_ref();
        let end = detail
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= MAX_DETAIL_BYTES)
            .last()
            .unwrap_or(0);
        let bounded = if detail.len() <= MAX_DETAIL_BYTES {
            detail.to_owned()
        } else {
            detail[..end].to_owned()
        };
        Self { kind, operation, recovery, detail: bounded }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> LinuxErrorKind {
        self.kind
    }
    /// Returns the stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }
    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> LinuxOperation {
        self.operation
    }
    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> LinuxRecovery {
        self.recovery
    }
    /// Returns bounded, non-sensitive detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub(crate) fn io(
        operation: LinuxOperation,
        context: &'static str,
        error: &std::io::Error,
    ) -> Self {
        Self::new(
            LinuxErrorKind::Io,
            operation,
            LinuxRecovery::ConfigureHost,
            format!("{context}: {error}"),
        )
    }
}

impl core::fmt::Display for LinuxError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)
    }
}

impl std::error::Error for LinuxError {}
