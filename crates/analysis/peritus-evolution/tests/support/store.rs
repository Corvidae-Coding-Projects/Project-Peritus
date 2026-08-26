use std::{path::PathBuf, time::Duration};

use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use super::bytes;

pub struct Stores {
    pub temporary: tempfile::TempDir,
    pub database: PathBuf,
    pub journal: SqliteJournal,
    pub artifacts: ArtifactStore,
    pub store_id: StoreId,
}

impl Stores {
    pub fn open() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let database = temporary.path().join("shared.sqlite3");
        let store_id = StoreId::new(bytes(80)).expect("journal store identity");
        let journal = open_journal(&database, store_id);
        let artifacts = ArtifactStore::open(
            StoreConfig::new(temporary.path().join("artifacts"), 1_048_576, 8 * 1_048_576)
                .expect("artifact configuration")
                .with_database_path(&database)
                .expect("shared artifact database"),
        )
        .expect("artifact store");
        Self { temporary, database, journal, artifacts, store_id }
    }
}

pub fn open_journal(path: &std::path::Path, store_id: StoreId) -> SqliteJournal {
    SqliteJournal::open(
        path,
        store_id,
        SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
    )
    .expect("journal")
}
