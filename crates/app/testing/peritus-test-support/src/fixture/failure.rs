//! Typed fixture validation failures.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// A stable category for canonical fixture failures.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FixtureErrorKind {
    /// A fixture name or version was invalid.
    InvalidName,
    /// A fixture-relative path was not portable and contained.
    InvalidPath,
    /// Reading filesystem state failed.
    Io,
    /// `fixture.toml` was not valid under the strict schema.
    ManifestSyntax,
    /// The fixture manifest schema number was unsupported.
    UnsupportedSchema,
    /// A SHA-256 value was not 64 lowercase hexadecimal bytes.
    InvalidDigest,
    /// Manifest file entries were not strictly sorted and unique.
    NonCanonicalFiles,
    /// A listed fixture file was missing.
    MissingFile,
    /// An unlisted file existed in the case directory.
    UnexpectedFile,
    /// A case path contained a symbolic link or non-regular file.
    UnsafeFileType,
    /// A listed file's bytes did not match its declared digest.
    DigestMismatch,
    /// Directory names disagreed with manifest identity fields.
    LayoutMismatch,
    /// Released compatibility evidence was empty.
    EmptyCatalog,
    /// A surface/version lacked a mandatory fixture kind.
    IncompleteCoverage,
}

impl FixtureErrorKind {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidName => "PERITUS-TEST-FIXTURE-001",
            Self::InvalidPath => "PERITUS-TEST-FIXTURE-002",
            Self::Io => "PERITUS-TEST-FIXTURE-003",
            Self::ManifestSyntax => "PERITUS-TEST-FIXTURE-004",
            Self::UnsupportedSchema => "PERITUS-TEST-FIXTURE-005",
            Self::InvalidDigest => "PERITUS-TEST-FIXTURE-006",
            Self::NonCanonicalFiles => "PERITUS-TEST-FIXTURE-007",
            Self::MissingFile => "PERITUS-TEST-FIXTURE-008",
            Self::UnexpectedFile => "PERITUS-TEST-FIXTURE-009",
            Self::UnsafeFileType => "PERITUS-TEST-FIXTURE-010",
            Self::DigestMismatch => "PERITUS-TEST-FIXTURE-011",
            Self::LayoutMismatch => "PERITUS-TEST-FIXTURE-012",
            Self::EmptyCatalog => "PERITUS-TEST-FIXTURE-013",
            Self::IncompleteCoverage => "PERITUS-TEST-FIXTURE-014",
        }
    }
}

/// A typed fixture failure with operation context and an optional source error.
#[derive(Debug)]
pub struct FixtureError {
    kind: FixtureErrorKind,
    path: Option<PathBuf>,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl FixtureError {
    pub(crate) fn new(kind: FixtureErrorKind, detail: impl Into<String>) -> Self {
        Self { kind, path: None, detail: detail.into(), source: None }
    }

    pub(crate) fn at(
        kind: FixtureErrorKind,
        path: impl Into<PathBuf>,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, path: Some(path.into()), detail: detail.into(), source: None }
    }

    pub(crate) fn sourced(
        kind: FixtureErrorKind,
        path: impl Into<PathBuf>,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            path: Some(path.into()),
            detail: detail.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> FixtureErrorKind {
        self.kind
    }

    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns the relevant path when the failure is path-specific.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Returns stable human-readable context without source formatting.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)?;
        if let Some(path) = &self.path {
            write!(formatter, " ({})", path.display())?;
        }
        Ok(())
    }
}

impl Error for FixtureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}
