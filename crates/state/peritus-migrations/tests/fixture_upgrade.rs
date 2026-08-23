//! Historical shared-database fixture upgrade and journal compatibility tests.

mod support;

use std::fs;

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_migrations::{MigrationEngine, MigrationRegistry, RecoveryState};

use support::{config, create_journal_database, operation, version};

#[test]
fn unversioned_fixture_upgrades_without_damaging_journal_replay_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database = create_journal_database(&temp);
    let connection = rusqlite::Connection::open(&database).expect("fixture connection");
    let current = std::env::current_dir().expect("migration test working directory");
    let fixture = current
        .ancestors()
        .map(|root| root.join("crates/state/peritus-migrations/fixtures/v0.sql"))
        .find(|path| path.is_file())
        .expect("checked-in v0 migration fixture path");
    let schema = fs::read_to_string(fixture).expect("read checked-in v0 migration fixture");
    connection.execute_batch(&schema).expect("install v0 fixture");
    drop(connection);

    let mut engine =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("migration engine");
    let plan = engine.preflight(version(1)).expect("preflight").into_plan();
    let applied = engine.apply(&plan, operation(9)).expect("upgrade fixture");
    assert!(applied.backup_path().expect("required backup").is_file());
    drop(engine);

    let fixture_connection = rusqlite::Connection::open(&database).expect("verify fixture");
    let payload: Vec<u8> = fixture_connection
        .query_row(
            "SELECT fixture_value FROM migration_fixture_payload WHERE fixture_key = 'preserved'",
            [],
            |row| row.get(0),
        )
        .expect("preserved fixture payload");
    assert_eq!(payload, b"peritus");
    drop(fixture_connection);

    let mut journal = SqliteJournal::open(
        &database,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("journal reopens after migration");
    let report = journal.integrity_scan().expect("journal integrity/replay scan");
    assert_eq!(report.event_count(), 0);
    assert_eq!(report.last_position(), 0);
    drop(journal);

    let mut rollback =
        MigrationEngine::open(config(&temp, database.clone()), MigrationRegistry::current())
            .expect("reopen migration engine for operational rollback");
    let restored = rollback.restore_backup(operation(9)).expect("restore historical fixture");
    assert_eq!(restored.state(), RecoveryState::Restored);
    drop(rollback);

    let restored_connection = rusqlite::Connection::open(&database).expect("restored fixture");
    let version: i64 = restored_connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("restored version");
    let restored_payload: Vec<u8> = restored_connection
        .query_row(
            "SELECT fixture_value FROM migration_fixture_payload WHERE fixture_key = 'preserved'",
            [],
            |row| row.get(0),
        )
        .expect("restored fixture payload");
    assert_eq!(version, 0);
    assert_eq!(restored_payload, b"peritus");
    drop(restored_connection);

    let mut restored_journal = SqliteJournal::open(
        database,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("journal reopens after rollback restoration");
    assert_eq!(restored_journal.integrity_scan().expect("restored integrity").event_count(), 0);
}
