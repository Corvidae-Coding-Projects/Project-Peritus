//! Immutable migration descriptor values.

use peritus_types::Sha256Digest;

use crate::{MigrationError, MigrationErrorCode, RecoveryClass};

/// Positive `SQLite` schema migration version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MigrationVersion(u64);

impl MigrationVersion {
    pub(crate) const FIRST: Self = Self(1);
    pub(crate) const SECOND: Self = Self(2);
    pub(crate) const THIRD: Self = Self(3);
    pub(crate) const FOURTH: Self = Self(4);
    pub(crate) const FIFTH: Self = Self(5);
    /// Creates a positive, `SQLite`-representable version.
    ///
    /// # Errors
    ///
    /// Returns an error for zero or values above [`i64::MAX`].
    pub const fn new(value: u64) -> Result<Self, MigrationError> {
        if value == 0 || value > i64::MAX as u64 {
            Err(MigrationError::message(
                MigrationErrorCode::InvalidRegistry,
                RecoveryClass::CorrectRequest,
                "validate migration version",
                "migration versions must be positive SQLite integers",
            ))
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the primitive version.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Whether a step requires a consistent pre-migration backup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupPolicy {
    /// Transaction rollback is sufficient for this step.
    NotRequired,
    /// A durable whole-file backup must complete before SQL runs.
    Required,
}

/// One immutable, statically compiled migration source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationDescriptor {
    version: MigrationVersion,
    release: &'static str,
    sql: &'static str,
    source_digest: Sha256Digest,
    backup: BackupPolicy,
    scratch_bytes: u64,
}

impl MigrationDescriptor {
    /// Declares an immutable migration and its reviewed exact-source digest.
    #[must_use]
    pub const fn new(
        version: MigrationVersion,
        release: &'static str,
        sql: &'static str,
        source_digest: Sha256Digest,
        backup: BackupPolicy,
        scratch_bytes: u64,
    ) -> Self {
        Self { version, release, sql, source_digest, backup, scratch_bytes }
    }

    /// Returns the target version installed by this step.
    #[must_use]
    pub const fn version(self) -> MigrationVersion {
        self.version
    }
    /// Returns the release first containing the migration.
    #[must_use]
    pub const fn release(self) -> &'static str {
        self.release
    }
    /// Returns exact reviewed SQL source bytes as UTF-8 text.
    #[must_use]
    pub const fn sql(self) -> &'static str {
        self.sql
    }
    /// Returns the expected exact-source SHA-256 digest.
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns backup policy.
    #[must_use]
    pub const fn backup_policy(self) -> BackupPolicy {
        self.backup
    }
    /// Returns declared temporary workspace bytes.
    #[must_use]
    pub const fn scratch_bytes(self) -> u64 {
        self.scratch_bytes
    }
}
