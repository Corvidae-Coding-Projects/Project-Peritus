//! Durable allocation of fresh command identities across runtime instances.
//!
//! Effect receipts decide whether a request may execute before shell allocation.
//! This allocator reserves numbers for admitted new work; it does not recover or
//! redispatch previous effects. Failed starts deliberately leave reserved gaps.

use std::{fs, path::Path, time::Duration};

use peritus_types::{ActionId, RunId};
use rusqlite::{Connection, TransactionBehavior, params};

use super::{contract, identity};

/// Reserves a number before any authority journal or process can be created.
/// `SQLite` serializes reservations across independently opened runtimes. Legacy
/// authority and compactor paths remain occupied even without an allocator row.
pub(super) fn reserve(root: &Path, run_id: RunId, after: u64) -> Result<u64, String> {
    let mut connection = Connection::open(root.join("command-ordinals.sqlite3"))
        .map_err(|error| detail("open store", &error))?;
    // Independent runtimes share this writer lock. Allow durable commits and
    // scheduling delays under contention without failing after only 250 ms.
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| detail("configure busy timeout", &error))?;
    // EXTRA also syncs the rollback-journal directory after commit. FULL alone
    // can lose the last acknowledged reservation on power loss in DELETE mode.
    connection
        .pragma_update(None, "synchronous", "EXTRA")
        .map_err(|error| detail("configure durable synchronization", &error))?;
    // Acquire the writer before installing the schema. First-open contenders
    // must not commit a separate schema transaction before reserving an ordinal.
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| detail("begin reservation", &error))?;
    transaction
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS command_ordinals (
            run_id BLOB PRIMARY KEY NOT NULL CHECK(length(run_id) = 16),
            last_ordinal INTEGER NOT NULL
                CHECK(typeof(last_ordinal) = 'integer' AND last_ordinal >= 0)
        );",
        )
        .map_err(|error| detail("install schema", &error))?;
    transaction
        .execute(
            "INSERT INTO command_ordinals(run_id, last_ordinal) VALUES (?1, 0)
         ON CONFLICT(run_id) DO NOTHING",
            params![run_id.as_bytes().as_slice()],
        )
        .map_err(|error| detail("initialize run counter", &error))?;
    let stored: i64 = transaction
        .query_row(
            "SELECT last_ordinal FROM command_ordinals WHERE run_id = ?1",
            params![run_id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| detail("read run counter", &error))?;
    let mut ordinal = u64::try_from(stored)
        .map_err(|_| "command ordinal store contains a negative number".to_owned())?
        .max(after);
    loop {
        ordinal = ordinal
            .checked_add(1)
            .filter(|value| i64::try_from(*value).is_ok())
            .ok_or_else(|| "command runtime action ordinal overflowed".to_owned())?;
        let action = ActionId::new(contract::id(run_id, ordinal, "action"))
            .map_err(|error| format!("construct reserved command identity: {error:?}"))?;
        let action = identity::action_hex(action);
        if !occupied(&root.join("authority").join(&action))?
            && !occupied(&root.join("local-compactor").join(&action))?
        {
            break;
        }
    }
    let stored = i64::try_from(ordinal)
        .map_err(|_| "command runtime action ordinal overflowed".to_owned())?;
    transaction
        .execute(
            "UPDATE command_ordinals SET last_ordinal = ?1 WHERE run_id = ?2",
            params![stored, run_id.as_bytes().as_slice()],
        )
        .map_err(|error| detail("update run counter", &error))?;
    // Never expose an allocation whose durable commit was not acknowledged.
    transaction.commit().map_err(|error| detail("commit reservation", &error))?;
    Ok(ordinal)
}

fn occupied(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect existing command identity: {error}")),
    }
}

fn detail(operation: &str, error: &rusqlite::Error) -> String {
    format!("reserve durable command ordinal ({operation}): {error}")
}
