//! `SQLite` connection ownership and hardened configuration.

use std::{path::Path, time::Duration};

use crate::{JournalError, JournalErrorKind, StoreId};
use rusqlite::{Connection, OpenFlags, config::DbConfig, limits::Limit, params};

/// `SQLite` connection configuration for a journal owner.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SqliteJournalOptions {
    /// Maximum time `SQLite` waits for a competing writer.
    pub busy_timeout: Duration,
}

impl Default for SqliteJournalOptions {
    fn default() -> Self {
        Self { busy_timeout: Duration::from_secs(5) }
    }
}

/// Observed safety-critical `SQLite` settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteSettings {
    /// Active journal mode, expected to be `wal` for file stores.
    pub journal_mode: String,
    /// `SQLite` synchronous level, expected to be `2` (`FULL`).
    pub synchronous: i64,
    /// Whether foreign-key enforcement is active.
    pub foreign_keys: bool,
    /// Configured busy timeout in milliseconds.
    pub busy_timeout_ms: u64,
    /// Whether defensive connection mode is active.
    pub defensive: bool,
}

/// Single-owner writable `SQLite` journal.
///
/// Mutating operations require `&mut self`, making the authoritative connection's serialized
/// ownership explicit. Separate values may still exercise `SQLite`'s real stale-CAS behavior.
pub struct SqliteJournal {
    pub(crate) connection: Connection,
    pub(crate) store_id: StoreId,
}

impl SqliteJournal {
    /// Opens or creates a file-backed journal and installs schema version three.
    ///
    /// # Errors
    ///
    /// Returns typed storage, schema, or store-identity errors.
    pub fn open(
        path: impl AsRef<Path>,
        store_id: StoreId,
        options: SqliteJournalOptions,
    ) -> Result<Self, JournalError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags)
            .map_err(|error| JournalError::sqlite("open journal", error))?;
        configure(&connection, options.busy_timeout)?;
        connection
            .execute_batch(super::schema::INSTALL_SCHEMA)
            .map_err(|error| JournalError::sqlite("install journal schema", error))?;
        peritus_artifact_store::sqlite_interop::install_schema(&connection)
            .map_err(|error| JournalError::sqlite("install artifact catalog schema", error))?;
        bind_store(&connection, store_id)?;
        Ok(Self { connection, store_id })
    }

    /// Returns the journal's exact store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Reads safety-critical connection settings.
    ///
    /// # Errors
    ///
    /// Returns a storage error if `SQLite` cannot report a setting.
    pub fn settings(&self) -> Result<SqliteSettings, JournalError> {
        let journal_mode = self
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read journal mode", error))?;
        let synchronous = self
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read synchronous mode", error))?;
        let foreign_keys: i64 = self
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read foreign keys", error))?;
        let busy_timeout_ms: i64 = self
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read busy timeout", error))?;
        let defensive = self
            .connection
            .db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
            .map_err(|error| JournalError::sqlite("read defensive mode", error))?;
        Ok(SqliteSettings {
            journal_mode,
            synchronous,
            foreign_keys: foreign_keys == 1,
            busy_timeout_ms: u64::try_from(busy_timeout_ms).map_err(|_| {
                JournalError::new(
                    JournalErrorKind::CorruptJournal,
                    "read busy timeout",
                    "SQLite returned a negative busy timeout",
                )
            })?,
            defensive,
        })
    }
}

fn configure(connection: &Connection, busy_timeout: Duration) -> Result<(), JournalError> {
    connection
        .busy_timeout(busy_timeout)
        .map_err(|error| JournalError::sqlite("configure busy timeout", error))?;
    let mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("configure WAL", error))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(JournalError::new(
            JournalErrorKind::Storage,
            "configure WAL",
            "SQLite did not activate WAL mode",
        ));
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| JournalError::sqlite("configure synchronous FULL", error))?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| JournalError::sqlite("configure foreign keys", error))?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|error| JournalError::sqlite("configure defensive mode", error))?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(|error| JournalError::sqlite("disable trusted schema", error))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 32 * 1024 * 1024)
        .map_err(|error| JournalError::sqlite("configure SQLite length limit", error))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(|error| JournalError::sqlite("disable attached databases", error))?;
    Ok(())
}

fn bind_store(connection: &Connection, store_id: StoreId) -> Result<(), JournalError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO store_meta(singleton, store_id, schema_version) VALUES (1, ?1, ?2)",
            params![store_id.as_bytes().as_slice(), super::schema::SCHEMA_VERSION],
        )
        .map_err(|error| JournalError::sqlite("bind store identity", error))?;
    let (stored, version): (Vec<u8>, i64) = connection
        .query_row(
            "SELECT store_id, schema_version FROM store_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| JournalError::sqlite("observe store identity", error))?;
    if stored.as_slice() != store_id.as_bytes() {
        return Err(JournalError::new(
            JournalErrorKind::InvalidInput,
            "open journal",
            "store identity does not match the existing database",
        ));
    }
    if version != super::schema::SCHEMA_VERSION {
        return Err(JournalError::new(
            JournalErrorKind::UnsupportedSchema,
            "open journal",
            "database schema version is unsupported",
        ));
    }
    Ok(())
}
