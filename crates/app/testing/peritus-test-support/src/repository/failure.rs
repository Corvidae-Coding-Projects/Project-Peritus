//! Typed temporary-repository failures.

use super::GitCommandOutput;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// Stable category for a temporary repository failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TempRepositoryErrorKind {
    /// The caller-selected owned root was broad, unsafe, or already existed.
    InvalidRoot,
    /// A filesystem operation failed.
    Io,
    /// Git could not be launched.
    GitSpawn,
    /// Git exited unsuccessfully.
    GitFailed,
    /// A worktree-only operation was requested for a bare repository.
    BareRepository,
    /// A contained path crossed a symlink or incompatible file type.
    UnsafePath,
    /// Git returned an invalid or unsupported object ID representation.
    InvalidObjectId,
    /// Git object-ID output was not UTF-8.
    NonUtf8ObjectId,
    /// Guarded recursive cleanup could not prove ownership or failed.
    Cleanup,
    /// Symbolic link creation is unavailable on this platform.
    SymlinkUnsupported,
}

impl TempRepositoryErrorKind {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRoot => "PERITUS-TEST-REPO-001",
            Self::Io => "PERITUS-TEST-REPO-002",
            Self::GitSpawn => "PERITUS-TEST-REPO-003",
            Self::GitFailed => "PERITUS-TEST-REPO-004",
            Self::BareRepository => "PERITUS-TEST-REPO-005",
            Self::UnsafePath => "PERITUS-TEST-REPO-006",
            Self::InvalidObjectId => "PERITUS-TEST-REPO-007",
            Self::NonUtf8ObjectId => "PERITUS-TEST-REPO-008",
            Self::Cleanup => "PERITUS-TEST-REPO-009",
            Self::SymlinkUnsupported => "PERITUS-TEST-REPO-010",
        }
    }
}

/// A typed repository failure with optional path, command output, and source error.
#[derive(Debug)]
pub struct TempRepositoryError {
    kind: TempRepositoryErrorKind,
    path: Option<PathBuf>,
    detail: String,
    output: Option<Box<GitCommandOutput>>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl TempRepositoryError {
    pub(crate) fn new(kind: TempRepositoryErrorKind, detail: impl Into<String>) -> Self {
        Self { kind, path: None, detail: detail.into(), output: None, source: None }
    }

    pub(crate) fn at(
        kind: TempRepositoryErrorKind,
        path: impl Into<PathBuf>,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, path: Some(path.into()), detail: detail.into(), output: None, source: None }
    }

    pub(crate) fn sourced(
        kind: TempRepositoryErrorKind,
        path: impl Into<PathBuf>,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            path: Some(path.into()),
            detail: detail.into(),
            output: None,
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn git_failed(output: GitCommandOutput) -> Self {
        Self {
            kind: TempRepositoryErrorKind::GitFailed,
            path: None,
            detail: format!("Git exited with status {}", output.status()),
            output: Some(Box::new(output)),
            source: None,
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> TempRepositoryErrorKind {
        self.kind
    }

    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns the relevant filesystem path when present.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Returns the complete Git observation for a nonzero exit.
    #[must_use]
    pub fn output(&self) -> Option<&GitCommandOutput> {
        self.output.as_deref()
    }

    /// Returns stable human-readable context without source formatting.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for TempRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)?;
        if let Some(path) = &self.path {
            write!(formatter, " ({})", path.display())?;
        }
        Ok(())
    }
}

impl Error for TempRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}
