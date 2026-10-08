//! Read-only inspection of existing journal state without schema installation or recovery.

use crate::{
    AggregateHead, AggregateKey, DurableStateRecord, JournalError, JournalErrorKind, SqliteJournal,
    OutboxId, OutboxMessage, StoreId,
};
use rusqlite::{Connection, OpenFlags, config::DbConfig, limits::Limit};
use std::{path::Path, sync::Arc};

/// A read-only `SQLite` snapshot with no mutation or initialization methods.
pub struct JournalReader {
    journal: SqliteJournal,
    history: SqliteJournal,
}

impl JournalReader {
    /// Opens an existing exact store at the current schema in a consistent read transaction.
    /// No migration, recovery, checkpoint, state installation, or file creation is requested.
    ///
    /// # Errors
    /// Rejects absent stores, identity/schema drift, and invalid database state.
    pub fn open(path: impl AsRef<Path>, store_id: StoreId) -> Result<Self, JournalError> {
        let path = path.as_ref();
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
            .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
            .map_err(|error| JournalError::sqlite("disable attached databases", error))?;
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
        let history_connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| JournalError::sqlite("open read-only journal history", error))?;
        history_connection
            .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(|error| JournalError::sqlite("harden read-only journal history", error))?;
        history_connection
            .pragma_update(None, "trusted_schema", false)
            .map_err(|error| JournalError::sqlite("disable trusted history schema", error))?;
        history_connection
            .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
            .map_err(|error| JournalError::sqlite("disable attached history databases", error))?;
        let (history_store, history_version): (Vec<u8>, i64) = history_connection
            .query_row(
                "SELECT store_id,schema_version FROM store_meta WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| JournalError::sqlite("read journal history identity", error))?;
        let history_migration: i64 = history_connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read history migration version", error))?;
        if history_store != store_id.as_bytes()
            || history_version != super::schema::SCHEMA_VERSION
            || history_migration != history_version
        {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "inspect existing journal history",
                "store identity or schema does not match",
            ));
        }
        let history_store_id = store_id;
        Ok(Self {
            journal: SqliteJournal {
                connection,
                store_id,
                replay_generation: Arc::new(()),
            },
            history: SqliteJournal {
                connection: history_connection,
                store_id: history_store_id,
                replay_generation: Arc::new(()),
            },
        })
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

    /// Reads and authenticates one immutable historical state revision from append-only history.
    ///
    /// # Errors
    /// Rejects invalid keys/revisions, corrupt history, or a history root that conflicts with the
    /// accepted current state.
    pub fn state_record_revision(
        &self,
        namespace: u16,
        key: &[u8],
        revision: u64,
    ) -> Result<Option<DurableStateRecord>, JournalError> {
        self.history.state_record_revision(namespace, key, revision)
    }

    /// Reads one exact aggregate head in the same snapshot.
    ///
    /// # Errors
    /// Rejects corrupt or unreadable journal data.
    pub fn head(&self, key: AggregateKey) -> Result<Option<AggregateHead>, JournalError> {
        self.journal.head(key)
    }

    /// Observes one exact outbox row in the same read snapshot without claiming it.
    ///
    /// # Errors
    /// Rejects corrupt or unreadable retained metadata and payload bytes.
    pub fn outbox_message(
        &self,
        id: OutboxId,
    ) -> Result<Option<OutboxMessage>, JournalError> {
        self.journal.outbox_message(id)
    }
}
