//! Stable migration errors and recovery classifications.

use std::{error::Error, fmt, io};

/// Stable machine-readable migration error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MigrationErrorCode {
    /// Configuration is invalid.
    InvalidConfiguration,
    /// The immutable migration registry is invalid.
    InvalidRegistry,
    /// Checked-in migration source differs from its declared digest.
    DigestDrift,
    /// Database or target version is unsupported.
    UnsupportedVersion,
    /// The running application does not support the version range.
    IncompatibleApplication,
    /// A requested transition would move backward.
    ForwardOnly,
    /// `SQLite` integrity verification failed.
    IntegrityCheckFailed,
    /// Available filesystem capacity is below the checked requirement.
    InsufficientSpace,
    /// Creating or validating a backup failed.
    BackupFailed,
    /// Applying migration SQL failed and rolled back.
    ApplyFailed,
    /// Explicit restoration from backup failed.
    RestoreFailed,
    /// Durable recovery metadata requires an explicit action.
    RecoveryRequired,
    /// Durable recovery metadata is inconsistent.
    RecoveryCorrupt,
    /// A database commit result must be reconciled after restart.
    Indeterminate,
    /// A deterministic test or reliability hook injected failure.
    InjectedFailure,
    /// A host I/O operation failed.
    Io,
    /// A `SQLite` operation failed outside migration SQL execution.
    Sqlite,
}

impl MigrationErrorCode {
    /// Returns the compatibility-stable textual code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "migration.invalid_configuration",
            Self::InvalidRegistry => "migration.invalid_registry",
            Self::DigestDrift => "migration.digest_drift",
            Self::UnsupportedVersion => "migration.unsupported_version",
            Self::IncompatibleApplication => "migration.incompatible_application",
            Self::ForwardOnly => "migration.forward_only",
            Self::IntegrityCheckFailed => "migration.integrity_check_failed",
            Self::InsufficientSpace => "migration.insufficient_space",
            Self::BackupFailed => "migration.backup_failed",
            Self::ApplyFailed => "migration.apply_failed",
            Self::RestoreFailed => "migration.restore_failed",
            Self::RecoveryRequired => "migration.recovery_required",
            Self::RecoveryCorrupt => "migration.recovery_corrupt",
            Self::Indeterminate => "migration.indeterminate",
            Self::InjectedFailure => "migration.injected_failure",
            Self::Io => "migration.io",
            Self::Sqlite => "migration.sqlite",
        }
    }
}

/// Recommended caller response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RecoveryClass {
    /// Correct inputs or compatibility configuration.
    CorrectRequest,
    /// Retry after the transient condition clears.
    Retry,
    /// Reopen and reconcile the durable operation identity.
    Reconcile,
    /// Explicitly restore the verified backup.
    RestoreBackup,
    /// Operator intervention is required; automatic retry is unsafe.
    Terminal,
}

/// Typed migration failure preserving host error sources.
#[derive(Debug)]
pub struct MigrationError {
    code: MigrationErrorCode,
    recovery: RecoveryClass,
    operation: &'static str,
    detail: Detail,
}

#[derive(Debug)]
enum Detail {
    Message(&'static str),
    Space { required: u64, available: u64 },
    Io(io::Error),
    Sqlite(rusqlite::Error),
}

impl MigrationError {
    pub(crate) const fn message(
        code: MigrationErrorCode,
        recovery: RecoveryClass,
        operation: &'static str,
        message: &'static str,
    ) -> Self {
        Self { code, recovery, operation, detail: Detail::Message(message) }
    }

    pub(crate) const fn space(required: u64, available: u64) -> Self {
        Self {
            code: MigrationErrorCode::InsufficientSpace,
            recovery: RecoveryClass::CorrectRequest,
            operation: "preflight capacity",
            detail: Detail::Space { required, available },
        }
    }

    pub(crate) const fn io(operation: &'static str, error: io::Error) -> Self {
        Self {
            code: MigrationErrorCode::Io,
            recovery: RecoveryClass::Retry,
            operation,
            detail: Detail::Io(error),
        }
    }

    pub(crate) const fn sqlite(operation: &'static str, error: rusqlite::Error) -> Self {
        Self {
            code: MigrationErrorCode::Sqlite,
            recovery: RecoveryClass::Retry,
            operation,
            detail: Detail::Sqlite(error),
        }
    }

    pub(crate) const fn migration_sql(error: rusqlite::Error) -> Self {
        Self {
            code: MigrationErrorCode::ApplyFailed,
            recovery: RecoveryClass::Reconcile,
            operation: "apply migration SQL",
            detail: Detail::Sqlite(error),
        }
    }

    pub(crate) const fn indeterminate_commit(error: rusqlite::Error) -> Self {
        Self {
            code: MigrationErrorCode::Indeterminate,
            recovery: RecoveryClass::Reconcile,
            operation: "commit migration transaction",
            detail: Detail::Sqlite(error),
        }
    }

    pub(crate) const fn backup(error: rusqlite::Error) -> Self {
        Self {
            code: MigrationErrorCode::BackupFailed,
            recovery: RecoveryClass::Retry,
            operation: "create consistent backup",
            detail: Detail::Sqlite(error),
        }
    }

    pub(crate) const fn restore(error: rusqlite::Error) -> Self {
        Self {
            code: MigrationErrorCode::RestoreFailed,
            recovery: RecoveryClass::RestoreBackup,
            operation: "restore consistent backup",
            detail: Detail::Sqlite(error),
        }
    }

    /// Returns the stable error code.
    #[must_use]
    pub const fn code(&self) -> MigrationErrorCode {
        self.code
    }

    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery_class(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns the stable operation label.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {}: ", self.code.as_str(), self.operation)?;
        match &self.detail {
            Detail::Message(message) => formatter.write_str(message),
            Detail::Space { required, available } => {
                write!(formatter, "required {required} bytes, available {available}")
            }
            Detail::Io(error) => error.fmt(formatter),
            Detail::Sqlite(error) => error.fmt(formatter),
        }
    }
}

impl Error for MigrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.detail {
            Detail::Io(error) => Some(error),
            Detail::Sqlite(error) => Some(error),
            Detail::Message(_) | Detail::Space { .. } => None,
        }
    }
}
