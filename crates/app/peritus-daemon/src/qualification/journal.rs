//! Shared authoritative-journal access for production qualification routes.

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery};

pub fn acquire_instance(
    config: &DaemonConfig,
    store_id: StoreId,
) -> Result<InstanceGuard, DaemonError> {
    InstanceGuard::acquire(config.paths().state_root(), &DaemonIdentity::new(store_id))
}

pub fn open_journal(
    config: &DaemonConfig,
    store_id: StoreId,
) -> Result<SqliteJournal, DaemonError> {
    SqliteJournal::open(config.paths().database(), store_id, SqliteJournalOptions::default())
        .map_err(journal_error)
}

pub fn verify_empty_journal(config: &DaemonConfig) -> Result<bool, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    verify_empty_journal_for_store(config, store_id)
}

pub fn verify_empty_journal_for_store(
    config: &DaemonConfig,
    store_id: StoreId,
) -> Result<bool, DaemonError> {
    let mut journal = open_journal(config, store_id)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    if report.event_count() != 0 || report.aggregate_count() != 0 || report.last_position() != 0 {
        return Err(DaemonError::new(
            DaemonErrorCode::CorruptState,
            DaemonRecovery::Operator,
            "qualify empty authoritative journal",
            "qualification changed the authoritative journal",
        ));
    }
    Ok(true)
}

pub fn journal_error(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}
