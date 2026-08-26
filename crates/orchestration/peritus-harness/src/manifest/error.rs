//! Typed strict-manifest and inventory failures.

use core::fmt;

/// Stable manifest-loading failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ManifestErrorKind {
    /// The manifest exceeded its compiled byte bound.
    ManifestTooLarge,
    /// The manifest was not strict UTF-8.
    InvalidUtf8,
    /// TOML syntax, field shape, or an unknown field was invalid.
    InvalidToml,
    /// The schema version was unsupported.
    UnsupportedSchema,
    /// A hexadecimal digest was malformed or noncanonical.
    InvalidDigest,
    /// A checked domain constructor rejected a declaration.
    InvalidDeclaration,
    /// C1 could not inspect the immutable workspace exactly.
    Workspace,
    /// C0 could not finalize or verify a declared artifact root.
    ArtifactStore,
    /// A required file or directory was absent.
    MissingEntry,
    /// Inventory contained a duplicate declaration or entry.
    DuplicateEntry,
    /// Inventory contained a file not declared by the manifest.
    UndeclaredEntry,
    /// A declared byte count disagreed with exact content.
    SizeMismatch,
    /// A declared content digest disagreed with exact content.
    DigestMismatch,
    /// A symlink or special entry was encountered.
    UnsafeEntry,
}

/// Comparable manifest failure with bounded path context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestError {
    kind: ManifestErrorKind,
    path: Option<String>,
    detail: String,
}

impl ManifestError {
    pub(super) fn new(kind: ManifestErrorKind, detail: impl Into<String>) -> Self {
        Self { kind, path: None, detail: detail.into() }
    }

    pub(super) fn at(
        kind: ManifestErrorKind,
        path: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, path: Some(path.into()), detail: detail.into() }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ManifestErrorKind {
        self.kind
    }

    /// Returns the affected canonical path, when present.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Returns bounded diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "harness manifest failed ({:?})", self.kind)?;
        if let Some(path) = &self.path {
            write!(formatter, " at {path}")?;
        }
        write!(formatter, ": {}", self.detail)
    }
}

impl std::error::Error for ManifestError {}

impl From<crate::domain::HarnessDomainError> for ManifestError {
    fn from(error: crate::domain::HarnessDomainError) -> Self {
        Self::new(ManifestErrorKind::InvalidDeclaration, error.to_string())
    }
}
