//! Read-only inspection of existing journal state without schema installation or recovery.

use crate::{
    AggregateHead, AggregateKey, DurableStateRecord, JournalError, JournalErrorKind, SqliteJournal,
    StoreId,
};
use rusqlite::{Connection, OpenFlags, config::DbConfig};
use std::path::Path;

/// A read-only `SQLite` snapshot with no mutation or initialization methods.
pub struct JournalReader {
    journal: SqliteJournal,
}

impl JournalReader {
    /// Opens an existing exact store at the current schema in a consistent read transaction.
    /// No migration, recovery, checkpoint, state installation, or file creation is requested.
    ///
    /// # Errors
    /// Rejects absent stores, identity/schema drift, and invalid database state.
    pub fn open(path: impl AsRef<Path>, store_id: StoreId) -> Result<Self, JournalError> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| JournalError::sqlite("open read-only journal", error))?;
        connection
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(|error| JournalError::sqlite("harden read-only journal", error))?;
        connection
            .pragma_update(None, "trusted_schema", false)
            .map_err(|error| JournalError::sqlite("disable trusted schema", error))?;
        connection
            .execute_batch("BEGIN DEFERRED;")
            .map_err(|error| JournalError::sqlite("begin journal read snapshot", error))?;
        let (stored, version): (Vec<u8>, i64) = connection
            .query_row(
                "SELECT store_id,schema_version FROM store_meta WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| JournalError::sqlite("read journal identity", error))?;
        let migration: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read migration version", error))?;
        if stored != store_id.as_bytes()
            || version != super::schema::SCHEMA_VERSION
            || migration != version
        {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "inspect existing journal",
                "store identity or schema does not match",
            ));
        }
        Ok(Self { journal: SqliteJournal { connection, store_id } })
    }

    /// Reads and digest-checks the current state row in this read snapshot.
    ///
    /// # Errors
    /// Rejects invalid keys, corrupt state, or missing producing events.
    pub fn state_record(
        &self,
        namespace: u16,
        key: &[u8],
    ) -> Result<Option<DurableStateRecord>, JournalError> {
        self.journal.state_record(namespace, key)
    }

    /// Reads one exact aggregate head in the same snapshot.
    ///
    /// # Errors
    /// Rejects corrupt or unreadable journal data.
    pub fn head(&self, key: AggregateKey) -> Result<Option<AggregateHead>, JournalError> {
        self.journal.head(key)
    }
}
