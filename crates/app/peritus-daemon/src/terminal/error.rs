//! Stable terminal-bridge failures with preserved protocol and process sources.

use core::fmt;

use peritus_app_protocol::TerminalError;
use peritus_process::ProcessError;

/// Stable failure category for live terminal registration and attachment operations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalBridgeErrorKind {
    /// A configured operational limit is zero or internally inconsistent.
    InvalidLimit,
    /// A bounded process or attachment registry is full.
    Capacity,
    /// The requested process has no live control registered in this daemon.
    ProcessNotRegistered,
    /// The authenticated actor or session does not own the process.
    OwnershipMismatch,
    /// A process or attachment identity conflicts with an existing registration.
    RegistrationConflict,
    /// The checked execution plan did not authorize a pseudo-terminal.
    NotPty,
    /// The exact operating-system process birth identity is unavailable.
    BirthIdentityUnavailable,
    /// A process event disagreed with the registered process or plan identity.
    ProcessIdentityMismatch,
    /// Required ordered output is no longer retained or was observed with a gap.
    ReplayUnavailable,
    /// A slow attachment exceeded its bounded pending-output allowance.
    Backpressure,
    /// The A3 attachment state rejected an operation.
    Protocol,
    /// C2 rejected a process control operation.
    Process,
    /// The process has already published its terminal result.
    ProcessNotLive,
}

/// Typed terminal bridge error.
#[derive(Debug)]
pub enum TerminalBridgeError {
    /// A bridge-owned validation or capacity rejection.
    Rejected {
        /// Stable public category.
        kind: TerminalBridgeErrorKind,
        /// Inert diagnostic text.
        detail: &'static str,
    },
    /// An A3 terminal-state rejection.
    Protocol(TerminalError),
    /// A C2 process-control rejection.
    Process(ProcessError),
}

impl TerminalBridgeError {
    /// Creates a bridge-owned rejection.
    #[must_use]
    pub(crate) const fn rejected(kind: TerminalBridgeErrorKind, detail: &'static str) -> Self {
        Self::Rejected { kind, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub(crate) const fn kind(&self) -> TerminalBridgeErrorKind {
        match self {
            Self::Rejected { kind, .. } => *kind,
            Self::Protocol(_) => TerminalBridgeErrorKind::Protocol,
            Self::Process(_) => TerminalBridgeErrorKind::Process,
        }
    }

    /// Returns inert diagnostic text.
    #[must_use]
    pub(crate) const fn detail(&self) -> &str {
        match self {
            Self::Rejected { detail, .. } => detail,
            Self::Protocol(error) => error.detail(),
            Self::Process(error) => error.detail(),
        }
    }
}

impl fmt::Display for TerminalBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind(), self.detail())
    }
}

impl std::error::Error for TerminalBridgeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected { .. } => None,
            Self::Protocol(error) => Some(error),
            Self::Process(error) => Some(error),
        }
    }
}

impl From<TerminalError> for TerminalBridgeError {
    fn from(error: TerminalError) -> Self {
        Self::Protocol(error)
    }
}

impl From<ProcessError> for TerminalBridgeError {
    fn from(error: ProcessError) -> Self {
        Self::Process(error)
    }
}
