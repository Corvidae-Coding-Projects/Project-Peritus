//! Store configuration validation.

use std::path::{Path, PathBuf};

use crate::{ArtifactStoreError, ErrorCode, RecoveryClass};

/// Validated policy and root configuration for one artifact store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreConfig {
    root: PathBuf,
    database_path: Option<PathBuf>,
    max_artifact_bytes: u64,
    quota_bytes: u64,
}

impl StoreConfig {
    /// Creates a store configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty root, zero bounds, or an artifact bound greater than quota.
    pub fn new(
        root: impl Into<PathBuf>,
        max_artifact_bytes: u64,
        quota_bytes: u64,
    ) -> Result<Self, ArtifactStoreError> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(invalid("the store root must not be empty"));
        }
        if max_artifact_bytes == 0 {
            return Err(invalid("the per-artifact byte limit must be positive"));
        }
        if quota_bytes == 0 {
            return Err(invalid("the store quota must be positive"));
        }
        if max_artifact_bytes > quota_bytes {
            return Err(invalid("the per-artifact byte limit exceeds the store quota"));
        }
        if quota_bytes > i64::MAX as u64 {
            return Err(invalid("the store quota exceeds durable SQLite accounting capacity"));
        }
        Ok(Self { root, database_path: None, max_artifact_bytes, quota_bytes })
    }

    /// Selects a caller-owned `SQLite` file shared with the authoritative journal.
    ///
    /// By default the store uses `metadata.sqlite3` below its root. A shared path lets journal
    /// appends and artifact-reference rows participate in the same `SQLite` transaction.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty database path.
    pub fn with_database_path(
        mut self,
        database_path: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactStoreError> {
        let database_path = database_path.into();
        if database_path.as_os_str().is_empty() {
            return Err(invalid("the artifact catalog database path must not be empty"));
        }
        self.database_path = Some(database_path);
        Ok(self)
    }

    /// Returns the configured, not-yet-canonicalized store root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the maximum bytes accepted by one writer.
    #[must_use]
    pub const fn max_artifact_bytes(&self) -> u64 {
        self.max_artifact_bytes
    }

    /// Returns the total logical byte quota used by checked quota plans.
    #[must_use]
    pub const fn quota_bytes(&self) -> u64 {
        self.quota_bytes
    }

    pub(crate) fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }
}

const fn invalid(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidConfiguration,
        RecoveryClass::CorrectRequest,
        message,
    )
}
