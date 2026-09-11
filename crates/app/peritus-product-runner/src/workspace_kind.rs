//! Caller-resolved workspace delivery boundary, independent of interaction mode.

use std::path::PathBuf;

/// Where the shared production pipeline inspects, edits, and verifies its result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ProductWorkspaceKind {
    /// Managed Git candidate with explicit acceptance/discard controls.
    #[default]
    Managed,
    /// Authorized in-place effects; scoped file evidence, no Git acceptance or rollback.
    InPlace {
        /// Private paths excluded from every model-facing role.
        protected_paths: Vec<PathBuf>,
        /// Immutable task scope epoch; retries retain it, completed follow-ups start a new one.
        baseline_revision: u64,
    },
}

impl ProductWorkspaceKind {
    /// Whether this is a caller-authorized in-place workspace.
    #[must_use]
    pub const fn is_in_place(&self) -> bool {
        matches!(self, Self::InPlace { .. })
    }

    /// Paths that must remain inaccessible to model workspace tools.
    #[must_use]
    pub fn protected_paths(&self) -> &[PathBuf] {
        match self {
            Self::Managed => &[],
            Self::InPlace { protected_paths, .. } => protected_paths,
        }
    }
}
