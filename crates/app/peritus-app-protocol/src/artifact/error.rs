//! Artifact-transfer rejection vocabulary.

use core::fmt;

/// Stable category for a rejected artifact transfer operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactTransferErrorKind {
    /// A configured transfer bound is zero or inconsistent.
    InvalidLimit,
    /// Metadata, media type, chunk, or terminal detail is malformed.
    InvalidInput,
    /// A chunk or cancellation names another transfer or artifact.
    BindingMismatch,
    /// A chunk ordinal is not the exact expected ordinal.
    UnexpectedOrdinal,
    /// A chunk offset is not the exact conserved byte count.
    UnexpectedOffset,
    /// An arithmetic operation overflowed or exceeded declared size.
    SizeOverflow,
    /// Completion was requested before exact declared-size conservation.
    Incomplete,
    /// The observed digest differs from declared metadata.
    DigestMismatch,
    /// A transition was attempted after terminal state.
    AlreadyTerminal,
    /// A repeated terminal fact conflicts with the retained fact.
    TerminalConflict,
}

/// Typed artifact transfer failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactTransferError {
    kind: ArtifactTransferErrorKind,
    detail: &'static str,
}

impl ArtifactTransferError {
    pub(crate) const fn new(kind: ArtifactTransferErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }

    /// Returns the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> ArtifactTransferErrorKind {
        self.kind
    }
    /// Returns inert diagnostic text.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for ArtifactTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for ArtifactTransferError {}

pub(super) const fn reject(
    kind: ArtifactTransferErrorKind,
    detail: &'static str,
) -> ArtifactTransferError {
    ArtifactTransferError::new(kind, detail)
}
