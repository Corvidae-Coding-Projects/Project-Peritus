//! Stable Git-tool failures and recovery guidance.

use core::fmt;
use std::borrow::Cow;

/// Stable Git-tool failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitToolErrorKind {
    /// Structured input is invalid or exceeds a hard bound.
    InvalidInput,
    /// Structured C1 Git observation failed.
    Git,
    /// Target-owned workspace authorization or effect failed.
    Workspace,
    /// Protocol catalog or rendering construction failed.
    Protocol,
}

impl GitToolErrorKind {
    /// Returns the compatibility-stable error code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "PERITUS-GIT-TOOL-001",
            Self::Git => "PERITUS-GIT-TOOL-002",
            Self::Workspace => "PERITUS-GIT-TOOL-003",
            Self::Protocol => "PERITUS-GIT-TOOL-004",
        }
    }
}

/// Git tool operation associated with a failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitToolOperation {
    /// Observe structured status.
    Status,
    /// Observe an immutable commit diff.
    Diff,
    /// Observe bounded history.
    History,
    /// Create a candidate and retained snapshot.
    Candidate,
    /// Inspect current or retained snapshot metadata.
    Snapshot,
    /// Restore a retained snapshot as a successor.
    Rollback,
    /// Deliver an approved merge to a user branch.
    Merge,
    /// Build or render the descriptor catalog.
    Catalog,
}

/// Required caller response to a Git-tool failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryClass {
    /// Correct structured input before retrying.
    CorrectInput,
    /// Re-observe the immutable repository/workspace.
    Reobserve,
    /// Retry unchanged semantic inputs with fresh action authority.
    Retry,
    /// Obtain fresh exact authority.
    Reauthorize,
    /// Quarantine requires authenticated human handling before retry.
    Quarantine,
    /// Reconcile a dirty or indeterminate workspace.
    Reconcile,
}

/// Bounded typed Git-tool error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitToolError {
    kind: GitToolErrorKind,
    source_code: Option<&'static str>,
    operation: GitToolOperation,
    recovery: RecoveryClass,
    detail: Cow<'static, str>,
}

impl GitToolError {
    pub(crate) const fn new(
        kind: GitToolErrorKind,
        operation: GitToolOperation,
        recovery: RecoveryClass,
        detail: &'static str,
    ) -> Self {
        Self { kind, source_code: None, operation, recovery, detail: Cow::Borrowed(detail) }
    }

    pub(crate) const fn invalid(operation: GitToolOperation, detail: &'static str) -> Self {
        Self::new(GitToolErrorKind::InvalidInput, operation, RecoveryClass::CorrectInput, detail)
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(&self) -> GitToolErrorKind {
        self.kind
    }
    /// Returns the lower-boundary code when one is available, otherwise the tool code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.source_code {
            Some(code) => code,
            None => self.kind.code(),
        }
    }

    pub(crate) const fn with_source_code(mut self, code: &'static str) -> Self {
        self.source_code = Some(code);
        self
    }
    pub(crate) fn with_source_detail(mut self, detail: &str) -> Self {
        let end = detail.floor_char_boundary(2048.min(detail.len()));
        let mut text: String = detail[..end]
            .chars()
            .map(|value| if value.is_control() { ' ' } else { value })
            .collect();
        if end < detail.len() {
            text.push_str(" [detail truncated]");
        }
        self.detail = Cow::Owned(text);
        self
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> GitToolOperation {
        self.operation
    }
    /// Returns the required recovery family.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }
    /// Returns bounded content-free detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for GitToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {:?}: {}", self.code(), self.operation, self.detail)
    }
}

impl std::error::Error for GitToolError {}
