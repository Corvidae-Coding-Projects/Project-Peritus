//! Store configuration validation.

use std::path::{Path, PathBuf};

use crate::{ArtifactStoreError, ErrorCode, RecoveryClass};

/// Capacity authority selected for one artifact store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoragePolicy {
    /// Durable logical bytes are bounded by an explicit caller-selected limit.
    LogicalQuota {
        /// Maximum sum of durable artifact record sizes.
        limit_bytes: u64,
        /// Unprivileged filesystem bytes retained after a strict reservation.
        minimum_free_bytes: u64,
    },
    /// Admission follows the filesystem that owns the store root.
    AvailableSpace {
        /// Unprivileged filesystem bytes retained after a strict reservation.
        minimum_free_bytes: u64,
    },
}

impl StoragePolicy {
    /// Returns the explicit logical limit, when logical quota owns admission.
    #[must_use]
    pub const fn logical_quota_bytes(self) -> Option<u64> {
        match self {
            Self::LogicalQuota { limit_bytes, .. } => Some(limit_bytes),
            Self::AvailableSpace { .. } => None,
        }
    }

    /// Returns the retained physical reserve applied alongside either admission policy.
    #[must_use]
    pub const fn minimum_free_bytes(self) -> u64 {
        match self {
            Self::LogicalQuota { minimum_free_bytes, .. }
            | Self::AvailableSpace { minimum_free_bytes } => minimum_free_bytes,
        }
    }
}

/// Validated policy and root configuration for one artifact store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreConfig {
    root: PathBuf,
    database_path: Option<PathBuf>,
    max_artifact_bytes: Option<u64>,
    quota_bytes: Option<u64>,
    minimum_free_bytes: u64,
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
        Ok(Self {
            root,
            database_path: None,
            max_artifact_bytes: Some(max_artifact_bytes),
            quota_bytes: Some(quota_bytes),
            minimum_free_bytes: 0,
        })
    }

    /// Creates a store governed by physical available space instead of cumulative lifetime bytes.
    ///
    /// Each temporary writer attempts best-effort physical preallocation by default. An explicit
    /// retained-free-space guarantee makes the capacity check and preallocation strict. The
    /// per-artifact bound is not a session quota.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty root, a zero bound, or a bound larger than one durable row.
    pub fn for_available_space(
        root: impl Into<PathBuf>,
        max_artifact_bytes: u64,
    ) -> Result<Self, ArtifactStoreError> {
        let root = root.into();
        validate_root_and_artifact_limit(&root, max_artifact_bytes)?;
        Ok(Self {
            root,
            database_path: None,
            max_artifact_bytes: Some(max_artifact_bytes),
            quota_bytes: None,
            minimum_free_bytes: 0,
        })
    }

    /// Creates a physical-capacity store without a synthetic per-artifact byte ceiling.
    ///
    /// Each writer still declares and verifies its exact expected size before publication. The
    /// filesystem-capacity policy decides whether that exact temporary allocation is admissible.
    ///
    /// # Errors
    /// Returns an error for an empty root.
    pub fn for_available_space_without_artifact_limit(
        root: impl Into<PathBuf>,
    ) -> Result<Self, ArtifactStoreError> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(invalid("the store root must not be empty"));
        }
        Ok(Self {
            root,
            database_path: None,
            max_artifact_bytes: None,
            quota_bytes: None,
            minimum_free_bytes: 0,
        })
    }

    /// Retains this many unprivileged filesystem bytes after each physical reservation.
    ///
    /// # Errors
    ///
    /// Returns an error when the reserve and maximum artifact cannot be represented together.
    pub fn with_minimum_free_bytes(
        mut self,
        minimum_free_bytes: u64,
    ) -> Result<Self, ArtifactStoreError> {
        if let Some(maximum) = self.max_artifact_bytes {
            maximum
                .checked_add(minimum_free_bytes)
                .ok_or_else(|| invalid("artifact allocation policy exceeds byte representation"))?;
        }
        self.minimum_free_bytes = minimum_free_bytes;
        Ok(self)
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
        match self.max_artifact_bytes {
            Some(value) => value,
            None => 0,
        }
    }

    /// Returns the optional configured per-artifact byte ceiling.
    #[must_use]
    pub const fn max_artifact_limit(&self) -> Option<u64> {
        self.max_artifact_bytes
    }

    /// Returns the optional total logical byte quota used by checked quota plans.
    #[must_use]
    pub const fn quota_bytes(&self) -> Option<u64> {
        self.quota_bytes
    }

    /// Returns the physical bytes retained after admitting a temporary artifact allocation.
    #[must_use]
    pub const fn minimum_free_bytes(&self) -> u64 {
        self.minimum_free_bytes
    }

    /// Returns the selected capacity authority and its exact configured bound.
    #[must_use]
    pub const fn storage_policy(&self) -> StoragePolicy {
        match self.quota_bytes {
            Some(limit_bytes) => StoragePolicy::LogicalQuota {
                limit_bytes,
                minimum_free_bytes: self.minimum_free_bytes,
            },
            None => StoragePolicy::AvailableSpace {
                minimum_free_bytes: self.minimum_free_bytes,
            },
        }
    }

    pub(crate) fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }
}

fn validate_root_and_artifact_limit(
    root: &Path,
    max_artifact_bytes: u64,
) -> Result<(), ArtifactStoreError> {
    if root.as_os_str().is_empty() {
        return Err(invalid("the store root must not be empty"));
    }
    if max_artifact_bytes == 0 {
        return Err(invalid("the per-artifact byte limit must be positive"));
    }
    if max_artifact_bytes > i64::MAX as u64 {
        return Err(invalid("the per-artifact byte limit exceeds durable SQLite representation"));
    }
    Ok(())
}

const fn invalid(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidConfiguration,
        RecoveryClass::CorrectRequest,
        message,
    )
}
