//! Empty-aggregate D2 replay behavior.

#![allow(clippy::unwrap_used, reason = "fixed replay fixture uses checked nonzero identities")]

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_review::load_review_replay;
use peritus_types::RunId;

#[test]
fn replay_of_an_absent_review_aggregate_is_explicitly_empty() {
    let directory = tempfile::tempdir().unwrap();
    let journal = SqliteJournal::open(
        directory.path().join("empty-review.sqlite3"),
        StoreId::new([1; 16]).unwrap(),
        SqliteJournalOptions::default(),
    )
    .unwrap();
    let replay = load_review_replay(&journal, RunId::new([2; 16]).unwrap()).unwrap();
    assert!(replay.events().is_empty());
    assert_eq!(replay.rebuild().unwrap(), None);
}
