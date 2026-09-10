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
/// SQLite serializes reservations across independently opened runtimes. Legacy
/// authority and compactor paths remain occupied even without an allocator row.
pub(super) fn reserve(root: &Path, run_id: RunId, after: u64) -> Result<u64, String> {
    let mut connection = Connection::open(root.join("command-ordinals.sqlite3")).map_err(detail)?;
    connection.busy_timeout(Duration::from_millis(250)).map_err(detail)?;
    connection.pragma_update(None, "synchronous", "FULL").map_err(detail)?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS command_ordinals (
            run_id BLOB PRIMARY KEY NOT NULL CHECK(length(run_id) = 16),
            last_ordinal INTEGER NOT NULL
                CHECK(typeof(last_ordinal) = 'integer' AND last_ordinal >= 0)
        );",
        )
        .map_err(detail)?;
    let transaction =
        connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(detail)?;
    transaction
        .execute(
            "INSERT INTO command_ordinals(run_id, last_ordinal) VALUES (?1, 0)
         ON CONFLICT(run_id) DO NOTHING",
            params![run_id.as_bytes().as_slice()],
        )
        .map_err(detail)?;
    let stored: i64 = transaction
        .query_row(
            "SELECT last_ordinal FROM command_ordinals WHERE run_id = ?1",
            params![run_id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(detail)?;
    let mut ordinal = u64::try_from(stored)
        .map_err(|_| "command ordinal store contains a negative number".to_owned())?
        .max(after);
    loop {
        ordinal = ordinal
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
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
        .map_err(detail)?;
    // Never expose an allocation whose durable commit was not acknowledged.
    transaction.commit().map_err(detail)?;
    Ok(ordinal)
}

fn occupied(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect existing command identity: {error}")),
    }
}

fn detail(error: rusqlite::Error) -> String {
    format!("reserve durable command ordinal: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_and_stale_local_counters_do_not_reuse_reservations() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([1; 16]).expect("run");
        assert_eq!(reserve(root.path(), run, 0).expect("first"), 1);
        // Every reserve opens a separate connection. No authority file is needed
        // for a reservation to survive a failed start or runtime restart.
        assert_eq!(reserve(root.path(), run, 0).expect("reopened"), 2);
        assert_eq!(reserve(root.path(), run, 1).expect("stale instance"), 3);
        assert_eq!(reserve(root.path(), run, 9).expect("local high water"), 10);
        assert_eq!(reserve(root.path(), run, 0).expect("reopened again"), 11);
    }

    #[test]
    fn legacy_shell_and_compactor_identities_are_preserved_including_gaps() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([2; 16]).expect("run");
        for (ordinal, directory) in [(1, "authority"), (3, "local-compactor")] {
            let action = ActionId::new(contract::id(run, ordinal, "action")).expect("action");
            let path = root.path().join(directory).join(identity::action_hex(action));
            fs::create_dir_all(&path).expect("legacy identity");
            fs::write(path.join("preserved"), b"legacy evidence").expect("evidence");
        }
        assert_eq!(reserve(root.path(), run, 0).expect("first free"), 2);
        assert_eq!(reserve(root.path(), run, 0).expect("skip later legacy"), 4);
        for (ordinal, directory) in [(1, "authority"), (3, "local-compactor")] {
            let action = ActionId::new(contract::id(run, ordinal, "action")).expect("action");
            let path = root.path().join(directory).join(identity::action_hex(action));
            assert_eq!(fs::read(path.join("preserved")).expect("retained"), b"legacy evidence");
        }
    }

    #[test]
    fn independent_threads_reserve_distinct_numbers() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([3; 16]).expect("run");
        assert_eq!(reserve(root.path(), run, 0).expect("initialize"), 1);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let path = root.path().to_path_buf();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    reserve(&path, run, 0).expect("concurrent allocation")
                })
            })
            .collect();
        let mut values: Vec<_> =
            handles.into_iter().map(|handle| handle.join().expect("allocation thread")).collect();
        values.sort_unstable();
        assert_eq!(values, vec![2, 3, 4, 5]);
    }

    #[test]
    fn malformed_store_and_exhaustion_fail_without_resetting_identity() {
        let root = tempfile::tempdir().expect("state directory");
        let run = RunId::new([4; 16]).expect("run");
        assert!(reserve(root.path(), run, u64::MAX).is_err());
        fs::write(root.path().join("command-ordinals.sqlite3"), b"invalid database")
            .expect("damaged fixture");
        assert!(reserve(root.path(), run, 0).is_err());
    }
}
