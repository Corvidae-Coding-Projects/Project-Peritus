//! Forward migration and exact durable-operation reconciliation.

use std::path::Path;

use peritus_migrations::{
    ApplicationCompatibility, MigrationConfig, MigrationEngine, MigrationOperationId,
    MigrationRegistry, RecoveryAction,
};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn migrate_existing(config: &DaemonConfig, database: &Path) -> Result<(), DaemonError> {
    let registry = MigrationRegistry::current();
    let latest = registry.latest().map_err(migration_error)?;
    let compatibility = ApplicationCompatibility::new(0, latest).map_err(migration_error)?;
    let migration_config = MigrationConfig::new(
        database,
        config.paths().backup_root(),
        env!("CARGO_PKG_VERSION"),
        compatibility,
        64 * 1_024 * 1_024,
    )
    .map_err(migration_error)?;
    let mut engine = MigrationEngine::open(migration_config, registry).map_err(migration_error)?;
    let operation = operation_id(registry)?;
    let _ = engine.adopt_current_install(operation).map_err(migration_error)?;
    let recovery = engine.reconcile().map_err(migration_error)?;
    for action in recovery.actions() {
        match *action {
            RecoveryAction::ReconciledApplied(_) => {}
            RecoveryAction::ResumeBackup(operation)
            | RecoveryAction::ResumeApply(operation)
            | RecoveryAction::RetryApply(operation) => {
                let plan = engine.preflight(latest).map_err(migration_error)?.into_plan();
                engine.apply(&plan, operation).map_err(migration_error)?;
            }
            RecoveryAction::RestoreBackup(_) => {
                return Err(DaemonError::new(
                    DaemonErrorCode::RecoveryRequired,
                    DaemonRecovery::Operator,
                    "reconcile schema migration",
                    "a verified pre-migration backup requires explicit operator restoration",
                ));
            }
        }
    }
    let plan = engine.preflight(latest).map_err(migration_error)?.into_plan();
    if plan.current_version() != latest.get() {
        engine.apply(&plan, operation).map_err(migration_error)?;
    }
    Ok(())
}

fn operation_id(registry: MigrationRegistry) -> Result<MigrationOperationId, DaemonError> {
    let digest = registry.digest().map_err(migration_error)?;
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    MigrationOperationId::new(bytes).map_err(migration_error)
}

fn migration_error(error: peritus_migrations::MigrationError) -> DaemonError {
    let recovery = match error.recovery_class() {
        peritus_migrations::RecoveryClass::CorrectRequest => DaemonRecovery::CorrectRequest,
        peritus_migrations::RecoveryClass::Retry => DaemonRecovery::Retry,
        peritus_migrations::RecoveryClass::Reconcile => DaemonRecovery::Reconcile,
        peritus_migrations::RecoveryClass::RestoreBackup
        | peritus_migrations::RecoveryClass::Terminal => DaemonRecovery::Operator,
        _ => DaemonRecovery::Operator,
    };
    let code = match error.code() {
        peritus_migrations::MigrationErrorCode::InvalidConfiguration
        | peritus_migrations::MigrationErrorCode::UnsupportedVersion
        | peritus_migrations::MigrationErrorCode::IncompatibleApplication
        | peritus_migrations::MigrationErrorCode::ForwardOnly
        | peritus_migrations::MigrationErrorCode::InsufficientSpace => {
            DaemonErrorCode::InvalidInput
        }
        peritus_migrations::MigrationErrorCode::Io
        | peritus_migrations::MigrationErrorCode::Sqlite
        | peritus_migrations::MigrationErrorCode::BackupFailed => DaemonErrorCode::Storage,
        peritus_migrations::MigrationErrorCode::RecoveryRequired
        | peritus_migrations::MigrationErrorCode::Indeterminate
        | peritus_migrations::MigrationErrorCode::ApplyFailed
        | peritus_migrations::MigrationErrorCode::RestoreFailed => {
            DaemonErrorCode::RecoveryRequired
        }
        peritus_migrations::MigrationErrorCode::InvalidRegistry
        | peritus_migrations::MigrationErrorCode::DigestDrift
        | peritus_migrations::MigrationErrorCode::IntegrityCheckFailed
        | peritus_migrations::MigrationErrorCode::RecoveryCorrupt
        | peritus_migrations::MigrationErrorCode::InjectedFailure => DaemonErrorCode::CorruptState,
        _ => DaemonErrorCode::CorruptState,
    };
    DaemonError::with_source(code, recovery, error.operation(), error.to_string(), error)
}
