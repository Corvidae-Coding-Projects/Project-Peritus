//! Migration engine configuration and application compatibility.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{MigrationError, MigrationErrorCode, MigrationVersion, RecoveryClass};

/// Database-version range supported by the running application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationCompatibility {
    minimum: u64,
    maximum: MigrationVersion,
}

impl ApplicationCompatibility {
    /// Creates a supported inclusive range; `minimum` may be zero for an unversioned legacy DB.
    ///
    /// # Errors
    ///
    /// Returns an error when the minimum exceeds the maximum.
    pub const fn new(minimum: u64, maximum: MigrationVersion) -> Result<Self, MigrationError> {
        if minimum > maximum.get() {
            Err(MigrationError::message(
                MigrationErrorCode::InvalidConfiguration,
                RecoveryClass::CorrectRequest,
                "validate compatibility",
                "minimum compatible version exceeds maximum",
            ))
        } else {
            Ok(Self { minimum, maximum })
        }
    }

    /// Returns minimum supported version.
    #[must_use]
    pub const fn minimum(self) -> u64 {
        self.minimum
    }
    /// Returns maximum supported version.
    #[must_use]
    pub const fn maximum(self) -> MigrationVersion {
        self.maximum
    }
}

/// Validated migration engine configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationConfig {
    database: PathBuf,
    backup_directory: PathBuf,
    application_release: String,
    compatibility: ApplicationCompatibility,
    space_reserve_bytes: u64,
    busy_timeout: Duration,
}

impl MigrationConfig {
    /// Creates migration configuration for an existing shared `SQLite` file.
    ///
    /// # Errors
    ///
    /// Returns an error for empty paths or invalid release text.
    pub fn new(
        database: impl Into<PathBuf>,
        backup_directory: impl Into<PathBuf>,
        application_release: impl Into<String>,
        compatibility: ApplicationCompatibility,
        space_reserve_bytes: u64,
    ) -> Result<Self, MigrationError> {
        let database = database.into();
        let backup_directory = backup_directory.into();
        let application_release = application_release.into();
        if database.as_os_str().is_empty() || backup_directory.as_os_str().is_empty() {
            return Err(invalid("database and backup directory must not be empty"));
        }
        if application_release.is_empty()
            || application_release.len() > 128
            || !application_release.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(invalid("application release must be bounded printable ASCII"));
        }
        Ok(Self {
            database,
            backup_directory,
            application_release,
            compatibility,
            space_reserve_bytes,
            busy_timeout: Duration::from_secs(5),
        })
    }

    /// Overrides `SQLite`'s bounded busy timeout.
    #[must_use]
    pub const fn with_busy_timeout(mut self, busy_timeout: Duration) -> Self {
        self.busy_timeout = busy_timeout;
        self
    }

    /// Returns database path.
    #[must_use]
    pub fn database(&self) -> &Path {
        &self.database
    }
    /// Returns backup directory.
    #[must_use]
    pub fn backup_directory(&self) -> &Path {
        &self.backup_directory
    }
    /// Returns application release.
    #[must_use]
    pub fn application_release(&self) -> &str {
        &self.application_release
    }
    /// Returns supported version range.
    #[must_use]
    pub const fn compatibility(&self) -> ApplicationCompatibility {
        self.compatibility
    }
    /// Returns capacity kept free beyond computed migration needs.
    #[must_use]
    pub const fn space_reserve_bytes(&self) -> u64 {
        self.space_reserve_bytes
    }
    /// Returns database busy timeout.
    #[must_use]
    pub const fn busy_timeout(&self) -> Duration {
        self.busy_timeout
    }
}

const fn invalid(message: &'static str) -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::InvalidConfiguration,
        RecoveryClass::CorrectRequest,
        "validate migration configuration",
        message,
    )
}
