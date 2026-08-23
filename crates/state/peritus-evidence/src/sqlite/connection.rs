//! Hardened shared-database connection policy.

use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};
use rusqlite::{Connection, OpenFlags, config::DbConfig, limits::Limit};
use std::{path::Path, time::Duration};

pub(super) fn open(path: &Path, busy_timeout: Duration) -> Result<Connection, EvidenceError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags)
        .map_err(|error| EvidenceError::sqlite("open evidence database", error))?;
    connection
        .busy_timeout(busy_timeout)
        .map_err(|error| EvidenceError::sqlite("set evidence busy timeout", error))?;
    let mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(|error| EvidenceError::sqlite("enable evidence WAL", error))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(EvidenceError::new(
            EvidenceErrorKind::Storage,
            RecoveryAction::RepairDependency,
            "enable evidence WAL",
            "SQLite did not activate WAL mode",
        ));
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| EvidenceError::sqlite("set evidence synchronous", error))?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| EvidenceError::sqlite("enable evidence foreign keys", error))?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|error| EvidenceError::sqlite("enable evidence defensive mode", error))?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(|error| EvidenceError::sqlite("disable evidence trusted schema", error))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 32 * 1024 * 1024)
        .map_err(|error| EvidenceError::sqlite("set evidence value limit", error))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(|error| EvidenceError::sqlite("disable evidence attached databases", error))?;
    Ok(connection)
}
