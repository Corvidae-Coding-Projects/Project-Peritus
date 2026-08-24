//! Stable Git adapter failures and recovery guidance.

use core::fmt;
use std::io;

/// Maximum stderr bytes retained on a failed Git invocation.
pub const MAX_ERROR_STDERR_BYTES: usize = 4_096;

/// Stable failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorKind {
    /// A caller-supplied value is outside the supported contract.
    InvalidInput,
    /// The selected path is not a valid Git repository.
    InvalidRepository,
    /// The repository uses an unsupported or contradictory feature.
    UnsupportedRepository,
    /// Git could not be launched.
    GitUnavailable,
    /// Git returned an unsuccessful status for a fixed operation.
    GitFailed,
    /// Git returned malformed, noncanonical, or excessive output.
    GitProtocol,
    /// An object ID is malformed, missing, or has the wrong type.
    ObjectMismatch,
    /// A worktree path or registration conflicts with existing state.
    WorktreeConflict,
    /// A worktree expected to be clean contains changes.
    DirtyWorktree,
    /// A snapshot reference already denotes another object.
    SnapshotConflict,
    /// Filesystem I/O failed.
    Io,
    /// The observed repository state cannot safely determine an outcome.
    Indeterminate,
}

impl ErrorKind {
    /// Returns the stable machine-readable error code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-GIT-001",
            Self::InvalidRepository => "PERITUS-GIT-002",
            Self::UnsupportedRepository => "PERITUS-GIT-003",
            Self::GitUnavailable => "PERITUS-GIT-004",
            Self::GitFailed => "PERITUS-GIT-005",
            Self::GitProtocol => "PERITUS-GIT-006",
            Self::ObjectMismatch => "PERITUS-GIT-007",
            Self::WorktreeConflict => "PERITUS-GIT-008",
            Self::DirtyWorktree => "PERITUS-GIT-009",
            Self::SnapshotConflict => "PERITUS-GIT-010",
            Self::Io => "PERITUS-GIT-011",
            Self::Indeterminate => "PERITUS-GIT-012",
        }
    }
}

/// Fixed adapter operation associated with an error.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Operation {
    /// Discover repository metadata.
    Discover,
    /// Resolve an immutable baseline.
    ResolveBaseline,
    /// Enumerate or inspect linked worktrees.
    InspectWorktree,
    /// Create a detached worktree.
    CreateWorktree,
    /// Remove an exact registered worktree.
    RemoveWorktree,
    /// Observe porcelain-v2 status.
    Status,
    /// Observe a structured immutable commit diff.
    Diff,
    /// Observe bounded first-parent-independent commit history.
    History,
    /// Write the isolated worktree index as a tree.
    CreateCandidate,
    /// Create or retain a snapshot commit/ref.
    CreateSnapshot,
    /// Restore an immutable snapshot tree.
    RestoreSnapshot,
    /// Release an exact snapshot reference.
    ReleaseSnapshot,
    /// Reconcile Git state with an expected tree.
    Reconcile,
    /// Decode and validate a persisted adapter manifest.
    DecodeManifest,
    /// Reopen a persisted worktree registration.
    ReopenWorktree,
    /// Reopen a persisted retained snapshot.
    ReopenSnapshot,
}

/// Recommended recovery family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct the request before retrying.
    CorrectRequest,
    /// Re-observe repository state and construct a fresh request.
    Reobserve,
    /// Run explicit workspace reconciliation.
    Reconcile,
    /// Retry may succeed without changing semantic inputs.
    Retry,
    /// Quarantine the affected workspace or repository.
    Quarantine,
}

/// Typed bounded failure from a structured Git operation.
#[derive(Debug)]
pub struct GitError {
    kind: ErrorKind,
    operation: Operation,
    recovery: RecoveryClass,
    detail: String,
    exit_status: Option<i32>,
    stderr: Vec<u8>,
    source: Option<io::Error>,
}

impl GitError {
    pub(crate) fn new(
        kind: ErrorKind,
        operation: Operation,
        recovery: RecoveryClass,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation,
            recovery,
            detail: detail.into(),
            exit_status: None,
            stderr: Vec::new(),
            source: None,
        }
    }

    pub(crate) fn command(operation: Operation, status: Option<i32>, stderr: &[u8]) -> Self {
        let recovery = match operation {
            Operation::CreateWorktree
            | Operation::RemoveWorktree
            | Operation::CreateCandidate
            | Operation::RestoreSnapshot => RecoveryClass::Reconcile,
            Operation::Discover
            | Operation::ResolveBaseline
            | Operation::InspectWorktree
            | Operation::Status
            | Operation::Diff
            | Operation::History
            | Operation::CreateSnapshot
            | Operation::ReleaseSnapshot
            | Operation::Reconcile
            | Operation::DecodeManifest
            | Operation::ReopenWorktree
            | Operation::ReopenSnapshot => RecoveryClass::Reobserve,
        };
        Self {
            kind: ErrorKind::GitFailed,
            operation,
            recovery,
            detail: "Git rejected the structured operation".to_owned(),
            exit_status: status,
            stderr: bounded(stderr),
            source: None,
        }
    }

    pub(crate) fn io(
        operation: Operation,
        recovery: RecoveryClass,
        detail: impl Into<String>,
        source: io::Error,
    ) -> Self {
        Self {
            kind: ErrorKind::Io,
            operation,
            recovery,
            detail: detail.into(),
            exit_status: None,
            stderr: Vec::new(),
            source: Some(source),
        }
    }

    pub(crate) fn unavailable(operation: Operation, source: io::Error) -> Self {
        Self {
            kind: ErrorKind::GitUnavailable,
            operation,
            recovery: RecoveryClass::Retry,
            detail: "the configured Git executable could not be launched".to_owned(),
            exit_status: None,
            stderr: Vec::new(),
            source: Some(source),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the fixed operation that failed.
    #[must_use]
    pub const fn operation(&self) -> Operation {
        self.operation
    }

    /// Returns the recommended recovery family.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns bounded explanatory text that excludes repository contents.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Returns the process exit code when Git exited normally.
    #[must_use]
    pub const fn exit_status(&self) -> Option<i32> {
        self.exit_status
    }

    /// Returns at most 4,096 bytes of exact Git stderr.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.kind.code(), self.operation, self.detail)
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source as _)
    }
}

fn bounded(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().copied().take(MAX_ERROR_STDERR_BYTES).collect()
}
