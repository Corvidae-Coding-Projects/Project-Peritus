//! Durable derived index for bounded pending-operation inspection.

use super::{OperationStore, RECORD_VERSION, Result, digest, problem, read_envelope, record_names};
use super::filesystem::sync_directory;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    path::PathBuf,
};

const INDEX_VERSION: i64 = 2;

impl OperationStore {
    /// Rebuilds this derived projection from authoritative envelopes when its database is absent,
    /// incompatible, or corrupt. Operation records are never moved or rewritten here.
    pub(super) fn recover_index(&self) -> Result<()> {
        let _owner = self.index_owner()?;
        let unresolved = self.authoritative_pending()?;
        if self
            .initialize_index_owned()
            .and_then(|()| self.reconcile_index_owned(&unresolved))
            .is_ok()
        {
            return Ok(());
        }
        self.preserve_broken_index()?;
        self.initialize_index_owned()?;
        self.reconcile_index_owned(&unresolved)
    }

    fn authoritative_pending(&self) -> Result<Vec<(String, String, bool)>> {
        let pending = self.root.join("pending");
        let mut records = Vec::new();
        for name in record_names(&pending)? {
            let path = pending.join(format!("{name}.json"));
            let envelope = read_envelope(&path)?
                .ok_or_else(|| problem("A pending operation disappeared during recovery"))?;
            if envelope.schema_version != RECORD_VERSION {
                return Err(problem("A pending operation uses an unsupported record schema"));
            }
            if digest(b"peritus/web/operation-record/v1\0", &envelope.operation) != name {
                return Err(problem("A pending operation record has the wrong durable identity"));
            }
            let unresolved = envelope.record.result.is_none();
            records.push((envelope.operation, name, unresolved));
        }
        Ok(records)
    }

    fn initialize_index_owned(&self) -> Result<()> {
        let mut connection = self.index_connection()?;
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(problem)?;
        if version == 0 || version == 1 {
            let generation = crate::state::id()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(problem)?;
            if version == 0 {
                transaction
                    .execute_batch(
                        "CREATE TABLE pending_operations (
                             sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                             operation TEXT NOT NULL UNIQUE,
                             digest TEXT NOT NULL UNIQUE
                         );",
                    )
                    .map_err(problem)?;
            }
            transaction
                .execute_batch(
                    "CREATE TABLE pending_index_metadata (
                         singleton INTEGER PRIMARY KEY CHECK(singleton=1),
                         generation TEXT NOT NULL
                     );",
                )
                .map_err(problem)?;
            transaction
                .execute(
                    "INSERT INTO pending_index_metadata(singleton,generation) VALUES (1,?1)",
                    [&generation],
                )
                .map_err(problem)?;
            transaction
                .pragma_update(None, "user_version", INDEX_VERSION)
                .map_err(problem)?;
            transaction.commit().map_err(problem)?;
            sync_directory(&self.root)?;
        } else if version != INDEX_VERSION {
            return Err(problem("The pending-operation index uses an unsupported schema"));
        }
        index_generation(&connection)?;
        Ok(())
    }

    fn reconcile_index_owned(&self, pending: &[(String, String, bool)]) -> Result<()> {
        let mut connection = self.index_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(problem)?;
        let mut authoritative = BTreeSet::new();
        let mut unresolved_records = BTreeSet::new();
        for (operation, digest, unresolved) in pending {
            authoritative.insert((operation.clone(), digest.clone()));
            if *unresolved {
                unresolved_records.insert((operation.clone(), digest.clone()));
                insert_exact(&transaction, operation, digest)?;
            }
        }
        let indexed = {
            let mut statement = transaction
                .prepare("SELECT operation,digest FROM pending_operations")
                .map_err(problem)?;
            let rows = statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(problem)?;
            let mut indexed = Vec::new();
            for row in rows {
                indexed.push(row.map_err(problem)?);
            }
            indexed
        };
        for (operation, retained_digest) in indexed {
            if digest(b"peritus/web/operation-record/v1\0", &operation) != retained_digest {
                return Err(problem("The pending-operation index has the wrong durable identity"));
            }
            if authoritative.contains(&(operation.clone(), retained_digest.clone())) {
                if unresolved_records.contains(&(operation.clone(), retained_digest.clone())) {
                    continue;
                }
            } else if super::read_record(
                &self.root.join("settled").join(format!("{retained_digest}.json")),
                &operation,
            )?
            .is_none()
            {
                // The index row is the only surviving evidence of this operation identity.
                // Preserve it as an explicit recovery hold instead of inferring lost input.
                continue;
            }
            transaction
                .execute(
                    "DELETE FROM pending_operations WHERE operation=?1 AND digest=?2",
                    params![operation, retained_digest],
                )
                .map_err(problem)?;
        }
        transaction.commit().map_err(problem)
    }

    fn preserve_broken_index(&self) -> Result<()> {
        let retained = self.root.join("index-recovery");
        std::fs::create_dir_all(&retained)?;
        let database = self.index_path();
        for source in [
            database.clone(),
            database.with_file_name("pending-index.sqlite3-journal"),
            database.with_file_name("pending-index.sqlite3-wal"),
            database.with_file_name("pending-index.sqlite3-shm"),
        ] {
            if !source.exists() {
                continue;
            }
            let name = source
                .file_name()
                .ok_or_else(|| problem("The pending-operation index has no file name"))?;
            let mut destination = retained.join(name);
            for suffix in 1_u64.. {
                if !destination.exists() {
                    break;
                }
                destination = retained.join(format!("{}.{}", name.to_string_lossy(), suffix));
            }
            std::fs::rename(&source, destination)?;
        }
        sync_directory(&retained)?;
        sync_directory(&self.root)
    }

    pub(super) fn ensure_pending_index(&self, operation: &str, digest: &str) -> Result<()> {
        let _owner = self.index_owner()?;
        self.index_pending(operation, digest)
    }

    pub(super) fn index_pending(&self, operation: &str, digest: &str) -> Result<()> {
        let mut connection = self.index_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(problem)?;
        insert_exact(&transaction, operation, digest)?;
        transaction.commit().map_err(problem)
    }

    pub(super) fn unindex_pending(&self, operation: &str, digest: &str) -> Result<()> {
        self.index_connection()?
            .execute(
                "DELETE FROM pending_operations WHERE operation=?1 AND digest=?2",
                params![operation, digest],
            )
            .map_err(problem)?;
        Ok(())
    }

    pub(super) fn has_pending_index(&self, operation: &str, digest: &str) -> Result<bool> {
        self.index_connection()?
            .query_row(
                "SELECT 1 FROM pending_operations WHERE operation=?1 AND digest=?2",
                params![operation, digest],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(problem)
    }

    pub(super) fn index_connection(&self) -> Result<Connection> {
        let connection = Connection::open(self.index_path()).map_err(problem)?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(problem)?;
        connection.busy_handler(Some(wait_for_index)).map_err(problem)?;
        Ok(connection)
    }

    pub(super) fn index_owner(&self) -> Result<File> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join("locks/index.lock"))?;
        fs4::FileExt::lock(&lock).map_err(problem)?;
        Ok(lock)
    }

    fn index_path(&self) -> PathBuf { self.root.join("pending-index.sqlite3") }
}

pub(super) fn index_generation(connection: &Connection) -> Result<String> {
    let generation = connection
        .query_row(
            "SELECT generation FROM pending_index_metadata WHERE singleton=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(problem)?;
    if generation.len() != 32
        || !generation.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(problem("The pending-operation index generation is invalid"));
    }
    Ok(generation)
}

fn wait_for_index(_attempt: i32) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(10));
    true
}

fn insert_exact(
    transaction: &rusqlite::Transaction<'_>,
    operation: &str,
    digest: &str,
) -> Result<()> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO pending_operations(operation,digest) VALUES (?1,?2)",
            params![operation, digest],
        )
        .map_err(problem)?;
    let retained = transaction
        .query_row(
            "SELECT digest FROM pending_operations WHERE operation=?1",
            [operation],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(problem)?;
    if retained.as_deref() != Some(digest) {
        return Err(problem(
            "The pending-operation index assigns an identity to another record",
        ));
    }
    Ok(())
}
