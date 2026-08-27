//! Single-owner migration engine and forward-only apply state machine.

use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags};

use crate::{
    MigrationConfig, MigrationError, MigrationErrorCode, MigrationOperationId, MigrationPlan,
    MigrationRegistry, MigrationVersion, PreflightReport, RecoveryClass, RecoveryState,
    SpaceObservation, backup, catalog, recovery::backup_path,
};

mod apply;
mod restart;

use apply::ApplyTransactionError;

/// Named reliability boundary available to deterministic fault injection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationFailpoint {
    /// Recovery identity is durable.
    AfterRecoveryRecord,
    /// Before consistent backup creation.
    BeforeBackup,
    /// Backup and its digest are durable.
    AfterBackup,
    /// Before one immutable migration step.
    BeforeStep(MigrationVersion),
    /// Before committing all selected migration steps.
    BeforeCommit,
    /// After schema commit but before acknowledgement/recovery-state update.
    AfterCommit,
}

/// Deterministic hook used by reliability tests and controlled fault campaigns.
pub trait MigrationHooks {
    /// Returns true to inject failure at this exact boundary.
    fn should_fail(&mut self, failpoint: MigrationFailpoint) -> bool;
}

/// Production hook implementation that never injects failure.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMigrationHooks;

impl MigrationHooks for NoMigrationHooks {
    fn should_fail(&mut self, _failpoint: MigrationFailpoint) -> bool {
        false
    }
}

/// Successful applied or idempotently reconciled migration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedMigration {
    operation: MigrationOperationId,
    from_version: u64,
    target_version: MigrationVersion,
    backup_path: Option<PathBuf>,
    reconciled: bool,
}

impl AppliedMigration {
    /// Returns operation identity.
    #[must_use]
    pub const fn operation(&self) -> MigrationOperationId {
        self.operation
    }
    /// Returns pre-migration version.
    #[must_use]
    pub const fn from_version(&self) -> u64 {
        self.from_version
    }
    /// Returns committed target version.
    #[must_use]
    pub const fn target_version(&self) -> MigrationVersion {
        self.target_version
    }
    /// Returns consistent backup path when backup policy required it.
    #[must_use]
    pub fn backup_path(&self) -> Option<&Path> {
        self.backup_path.as_deref()
    }
    /// Returns whether restart reconciliation, rather than the original acknowledgement, observed
    /// the committed result.
    #[must_use]
    pub const fn reconciled(&self) -> bool {
        self.reconciled
    }
}

/// Exclusive owner of migration I/O for one shared `SQLite` file.
pub struct MigrationEngine {
    connection: Connection,
    _owner_lock: fs::File,
    config: MigrationConfig,
    registry: MigrationRegistry,
    database: PathBuf,
    backup_directory: PathBuf,
}

impl MigrationEngine {
    /// Opens an existing `SQLite` file, installs only migration-owned tables, and configures
    /// durable `SQLite` behavior.
    ///
    /// # Errors
    ///
    /// Returns configuration, filesystem, `SQLite`, or registry errors.
    pub fn open(
        config: MigrationConfig,
        registry: MigrationRegistry,
    ) -> Result<Self, MigrationError> {
        registry.validate()?;
        let metadata = fs::symlink_metadata(config.database())
            .map_err(|error| MigrationError::io("inspect migration database", error))?;
        if !metadata.file_type().is_file() {
            return Err(invalid_config("migration database must be a regular file"));
        }
        let database = fs::canonicalize(config.database())
            .map_err(|error| MigrationError::io("canonicalize migration database", error))?;
        fs::create_dir_all(config.backup_directory())
            .map_err(|error| MigrationError::io("create backup directory", error))?;
        let backup_directory = fs::canonicalize(config.backup_directory())
            .map_err(|error| MigrationError::io("canonicalize backup directory", error))?;
        if database.starts_with(&backup_directory) {
            return Err(invalid_config("database file must not be inside backup directory"));
        }
        let owner_lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(backup_directory.join("migration-owner.lock"))
            .map_err(|error| MigrationError::io("open migration owner lock", error))?;
        fs4::FileExt::try_lock(&owner_lock).map_err(|error| {
            MigrationError::io("acquire exclusive migration ownership", error.into())
        })?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(&database, flags)
            .map_err(|error| MigrationError::sqlite("open migration database", error))?;
        connection
            .busy_timeout(config.busy_timeout())
            .map_err(|error| MigrationError::sqlite("configure migration busy timeout", error))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| MigrationError::sqlite("configure synchronous FULL", error))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| MigrationError::sqlite("configure foreign keys", error))?;
        verify_database(&connection)?;
        catalog::install(&connection)?;
        Ok(Self {
            connection,
            _owner_lock: owner_lock,
            config,
            registry,
            database,
            backup_directory,
        })
    }

    /// Runs registry/history validation, `SQLite` integrity checking, compatibility checks, and
    /// deterministic capacity planning.
    ///
    /// # Errors
    ///
    /// Returns registry drift, corruption, incompatibility, arithmetic, space, or `SQLite` errors.
    pub fn preflight(&self, target: MigrationVersion) -> Result<PreflightReport, MigrationError> {
        verify_database(&self.connection)?;
        let current = catalog::current_version(&self.connection, self.registry)?;
        let database_bytes = logical_database_bytes(&self.connection)?;
        let database_available_bytes = fs4::available_space(&self.database)
            .map_err(|error| MigrationError::io("observe database free space", error))?;
        let backup_available_bytes = fs4::available_space(&self.backup_directory)
            .map_err(|error| MigrationError::io("observe backup free space", error))?;
        let available_bytes = database_available_bytes.min(backup_available_bytes);
        let plan = MigrationPlan::build(
            self.registry,
            current,
            target,
            self.config.compatibility(),
            database_bytes,
            available_bytes,
            self.config.space_reserve_bytes(),
        )?;
        Ok(PreflightReport::new(
            plan,
            SpaceObservation::new(database_bytes, database_available_bytes, backup_available_bytes),
        ))
    }

    /// Atomically records the compiled migration history for a directly installed current C0
    /// schema.
    ///
    /// This bridge is only applicable when the journal's own schema metadata and `SQLite`
    /// `user_version` already equal the registry's latest version, no migration history exists,
    /// and no recovery operation is present. Existing migration-managed databases are unchanged.
    ///
    /// # Errors
    ///
    /// Rejects incomplete, mismatched, or recovery-bearing direct installations.
    pub fn adopt_current_install(
        &mut self,
        operation: MigrationOperationId,
    ) -> Result<bool, MigrationError> {
        verify_database(&self.connection)?;
        catalog::adopt_current_install(&mut self.connection, self.registry, operation)
    }

    /// Applies a preflighted plan using production hooks.
    ///
    /// # Errors
    ///
    /// Returns plan drift, backup, apply, indeterminate commit, or recovery-required errors.
    pub fn apply(
        &mut self,
        plan: &MigrationPlan,
        operation: MigrationOperationId,
    ) -> Result<AppliedMigration, MigrationError> {
        self.apply_with_hooks(plan, operation, &mut NoMigrationHooks)
    }

    /// Applies a preflighted plan with explicit deterministic reliability hooks.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::apply`] plus injected failures.
    pub fn apply_with_hooks<H: MigrationHooks>(
        &mut self,
        plan: &MigrationPlan,
        operation: MigrationOperationId,
        hooks: &mut H,
    ) -> Result<AppliedMigration, MigrationError> {
        self.validate_plan(plan)?;
        let stored = catalog::begin_operation(
            &self.connection,
            operation,
            plan,
            self.config.application_release(),
        )?;
        if stored.state() == RecoveryState::Applied {
            return Ok(self.applied(plan, operation, true));
        }
        if matches!(stored.state(), RecoveryState::RestoreRequired | RecoveryState::Restored) {
            return Err(recovery_required(
                "operation requires backup restoration or a new identity",
            ));
        }
        fail(hooks, MigrationFailpoint::AfterRecoveryRecord)?;

        let path = backup_path(&self.backup_directory, operation, plan.current_version());
        let mut backup_digest = stored.backup_digest();
        if plan.backup_required() && stored.state() == RecoveryState::Planned {
            fail(hooks, MigrationFailpoint::BeforeBackup)?;
            let digest = backup::create(&self.connection, &path)?;
            catalog::update_state(
                &self.connection,
                operation,
                RecoveryState::BackupReady,
                Some(digest),
                None,
            )?;
            backup_digest = Some(digest);
            fail(hooks, MigrationFailpoint::AfterBackup)?;
        }
        if plan.backup_required() {
            let digest = backup_digest.ok_or_else(|| {
                crate::recovery::corrupt("backup-ready operation has no durable backup digest")
            })?;
            backup::verify(&path, digest)?;
        }
        catalog::update_state(&self.connection, operation, RecoveryState::Applying, None, None)?;
        self.connection
            .pragma_update(None, "foreign_keys", false)
            .map_err(|error| MigrationError::sqlite("suspend foreign keys for migration", error))?;
        let apply_result = self.apply_transaction(plan, operation, hooks);
        self.connection.pragma_update(None, "foreign_keys", true).map_err(|error| {
            MigrationError::sqlite("restore foreign keys after migration", error)
        })?;
        match apply_result {
            Ok(()) => {}
            Err(ApplyTransactionError::BeforeCommit(error)) => {
                let state = if plan.backup_required() {
                    RecoveryState::RestoreRequired
                } else {
                    RecoveryState::Failed
                };
                catalog::update_state(
                    &self.connection,
                    operation,
                    state,
                    None,
                    Some(error.code().as_str()),
                )?;
                return Err(error);
            }
            Err(ApplyTransactionError::CommitIndeterminate(error)) => return Err(error),
        }
        if let Err(error) = verify_database(&self.connection) {
            let state = if plan.backup_required() {
                RecoveryState::RestoreRequired
            } else {
                RecoveryState::Failed
            };
            catalog::update_state(
                &self.connection,
                operation,
                state,
                None,
                Some(error.code().as_str()),
            )?;
            return Err(error);
        }
        catalog::update_state(&self.connection, operation, RecoveryState::Applied, None, None)?;
        Ok(self.applied(plan, operation, false))
    }

    fn validate_plan(&self, plan: &MigrationPlan) -> Result<(), MigrationError> {
        if plan.registry_digest() != self.registry.digest()?
            || plan.current_version() != catalog::current_version(&self.connection, self.registry)?
        {
            return Err(MigrationError::message(
                MigrationErrorCode::RecoveryRequired,
                RecoveryClass::Reconcile,
                "validate migration plan",
                "preflight plan no longer matches registry or durable current version",
            ));
        }
        Ok(())
    }

    fn applied(
        &self,
        plan: &MigrationPlan,
        operation: MigrationOperationId,
        reconciled: bool,
    ) -> AppliedMigration {
        AppliedMigration {
            operation,
            from_version: plan.current_version(),
            target_version: plan.target_version(),
            backup_path: plan
                .backup_required()
                .then(|| backup_path(&self.backup_directory, operation, plan.current_version())),
            reconciled,
        }
    }
}

fn verify_integrity(connection: &Connection) -> Result<(), MigrationError> {
    let result: String =
        connection
            .pragma_query_value(None, "integrity_check", |row| row.get(0))
            .map_err(|error| MigrationError::sqlite("run SQLite integrity check", error))?;
    if result != "ok" {
        return Err(MigrationError::message(
            MigrationErrorCode::IntegrityCheckFailed,
            RecoveryClass::Terminal,
            "run SQLite integrity check",
            "SQLite integrity_check did not return ok",
        ));
    }
    Ok(())
}

fn verify_database(connection: &Connection) -> Result<(), MigrationError> {
    verify_integrity(connection)?;
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|error| MigrationError::sqlite("prepare SQLite foreign-key check", error))?;
    let has_failure = statement
        .exists([])
        .map_err(|error| MigrationError::sqlite("run SQLite foreign-key check", error))?;
    if has_failure {
        return Err(MigrationError::message(
            MigrationErrorCode::IntegrityCheckFailed,
            RecoveryClass::Terminal,
            "run SQLite foreign-key check",
            "SQLite foreign_key_check reported a violation",
        ));
    }
    Ok(())
}

fn logical_database_bytes(connection: &Connection) -> Result<u64, MigrationError> {
    let pages: i64 = connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(|error| MigrationError::sqlite("observe SQLite page count", error))?;
    let page_size: i64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(|error| MigrationError::sqlite("observe SQLite page size", error))?;
    let pages =
        u64::try_from(pages).map_err(|_| crate::recovery::corrupt("negative page count"))?;
    let page_size =
        u64::try_from(page_size).map_err(|_| crate::recovery::corrupt("negative page size"))?;
    pages
        .checked_mul(page_size)
        .ok_or_else(|| crate::recovery::corrupt("logical database byte count overflowed"))
}

fn fail<H: MigrationHooks>(
    hooks: &mut H,
    failpoint: MigrationFailpoint,
) -> Result<(), MigrationError> {
    if hooks.should_fail(failpoint) {
        Err(MigrationError::message(
            MigrationErrorCode::InjectedFailure,
            RecoveryClass::Reconcile,
            "migration failpoint",
            "deterministic reliability hook injected failure",
        ))
    } else {
        Ok(())
    }
}

const fn invalid_config(message: &'static str) -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::InvalidConfiguration,
        RecoveryClass::CorrectRequest,
        "open migration engine",
        message,
    )
}

const fn recovery_required(message: &'static str) -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::RecoveryRequired,
        RecoveryClass::Reconcile,
        "resume migration operation",
        message,
    )
}
