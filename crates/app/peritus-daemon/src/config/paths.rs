//! Protected daemon filesystem-root configuration.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::DaemonError;

use super::invalid;

/// Protected daemon filesystem roots.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DaemonPaths {
    state_root: PathBuf,
    artifact_root: PathBuf,
    evidence_root: PathBuf,
    workspace_root: PathBuf,
    process_root: PathBuf,
    transaction_root: PathBuf,
    backup_root: PathBuf,
}

impl DaemonPaths {
    /// Creates and validates absolute, lexically normalized daemon roots.
    ///
    /// # Errors
    ///
    /// Returns invalid input for relative paths, parent traversal, or duplicate roots.
    pub fn new(
        state_root: PathBuf,
        artifact_root: PathBuf,
        evidence_root: PathBuf,
        workspace_root: PathBuf,
        process_root: PathBuf,
        transaction_root: PathBuf,
        backup_root: PathBuf,
    ) -> Result<Self, DaemonError> {
        let paths = Self {
            state_root,
            artifact_root,
            evidence_root,
            workspace_root,
            process_root,
            transaction_root,
            backup_root,
        };
        paths.validate()?;
        Ok(paths)
    }

    /// Returns the protected daemon state root.
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }
    /// Returns the immutable artifact root.
    #[must_use]
    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }
    /// Returns the acceptance-evidence root.
    #[must_use]
    pub fn evidence_root(&self) -> &Path {
        &self.evidence_root
    }
    /// Returns the registered-workspace parent.
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
    /// Returns the native process registry root.
    #[must_use]
    pub fn process_root(&self) -> &Path {
        &self.process_root
    }
    /// Returns the C1 mutation transaction parent.
    #[must_use]
    pub fn transaction_root(&self) -> &Path {
        &self.transaction_root
    }
    /// Returns the migration backup root.
    #[must_use]
    pub fn backup_root(&self) -> &Path {
        &self.backup_root
    }
    /// Returns the shared SQLite database path.
    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.state_root.join("peritus.sqlite3")
    }

    pub(super) fn validate(&self) -> Result<(), DaemonError> {
        let children = [
            &self.artifact_root,
            &self.evidence_root,
            &self.workspace_root,
            &self.process_root,
            &self.transaction_root,
            &self.backup_root,
        ];
        for path in std::iter::once(&self.state_root).chain(children) {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
            {
                return Err(invalid(
                    "daemon paths must be absolute and contain no parent traversal",
                ));
            }
        }
        for child in children {
            if !child.starts_with(&self.state_root) || child == &self.state_root {
                return Err(invalid("daemon protected component roots must be beneath state_root"));
            }
        }
        for (index, left) in children.iter().enumerate() {
            if children
                .iter()
                .skip(index + 1)
                .any(|right| left.starts_with(right.as_path()) || right.starts_with(left.as_path()))
            {
                return Err(invalid("daemon protected component roots must not overlap"));
            }
        }
        Ok(())
    }
}
