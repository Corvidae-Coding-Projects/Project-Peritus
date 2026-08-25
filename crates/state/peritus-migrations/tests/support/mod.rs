#![allow(dead_code)]

use std::path::PathBuf;

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_migrations::{
    ApplicationCompatibility, MigrationConfig, MigrationEngine, MigrationOperationId,
    MigrationRegistry, MigrationVersion,
};
use tempfile::TempDir;

pub fn version(value: u64) -> MigrationVersion {
    MigrationVersion::new(value).expect("positive test migration version")
}

pub fn operation(value: u8) -> MigrationOperationId {
    MigrationOperationId::new([value; 16]).expect("nonzero operation identity")
}

pub fn create_database(temp: &TempDir) -> PathBuf {
    let path = temp.path().join("journal.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("create SQLite fixture");
    connection.execute_batch("PRAGMA journal_mode = WAL;").expect("enable WAL");
    drop(connection);
    path
}

pub fn create_journal_database(temp: &TempDir) -> PathBuf {
    let path = temp.path().join("journal.sqlite3");
    let journal = SqliteJournal::open(
        &path,
        StoreId::new([1; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("create shared journal database");
    drop(journal);
    path
}

pub fn config(temp: &TempDir, database: PathBuf) -> MigrationConfig {
    MigrationConfig::new(
        database,
        temp.path().join("backups"),
        "test-release",
        ApplicationCompatibility::new(0, version(4)).expect("compatibility"),
        0,
    )
    .expect("migration config")
}

pub fn engine(temp: &TempDir) -> MigrationEngine {
    let database = create_database(temp);
    MigrationEngine::open(config(temp, database), MigrationRegistry::current())
        .expect("migration engine")
}
