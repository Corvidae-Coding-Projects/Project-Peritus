use peritus_codec::{CodecLimits, encode_frame};
use peritus_types::{EventSequence, Sha256Digest};
use rusqlite::params;
use tempfile::TempDir;

use crate::{AggregateKind, AppendRequest, EventDraft, ExactFrame, HeadExpectation};

use super::{command, event, key, open, store_id};

#[test]
fn rejected_open_does_not_leave_partial_schema_installation() {
    for (identity, version, kind) in [
        ([2_u8; 16], 1, crate::JournalErrorKind::InvalidInput),
        ([1_u8; 16], 99, crate::JournalErrorKind::UnsupportedSchema),
    ] {
        let temp = TempDir::new().expect("temporary directory");
        let path = temp.path().join("partial.sqlite3");
        let connection = rusqlite::Connection::open(&path).expect("fixture");
        connection
            .execute_batch(
                "CREATE TABLE store_meta(singleton INTEGER PRIMARY KEY, store_id BLOB NOT NULL,
                                     schema_version INTEGER NOT NULL) STRICT;",
            )
            .expect("pre-existing store metadata");
        connection
            .execute("INSERT INTO store_meta VALUES (1, ?1, ?2)", params![identity, version])
            .expect("rejected identity or version");
        let before = schema_objects(&connection);
        drop(connection);
        let error =
            crate::SqliteJournal::open(&path, store_id(), crate::SqliteJournalOptions::default())
                .err()
                .expect("reject incompatible store binding");
        assert_eq!(error.kind(), kind);
        let connection = rusqlite::Connection::open(&path).expect("inspect rejected open");
        assert_eq!(schema_objects(&connection), before);
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .expect("version unchanged"),
            0
        );
    }
}

fn schema_objects(connection: &rusqlite::Connection) -> Vec<(String, String)> {
    connection
        .prepare("SELECT name, sql FROM sqlite_schema ORDER BY name")
        .expect("schema statement")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("schema rows")
        .collect::<Result<_, _>>()
        .expect("schema values")
}

#[test]
fn page_ceiling_returns_exact_storage_exhaustion_without_partial_append() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let baseline = journal.storage_pages().expect("storage pages");
    let limited =
        journal.limit_storage_pages(baseline.page_count()).expect("limit to current pages");
    assert_eq!(limited.maximum_pages(), baseline.page_count());
    assert_eq!(limited.maximum_bytes(), limited.maximum_pages() * limited.page_size());

    let aggregate = key(AggregateKind::Kernel, 11);
    let frame = ExactFrame::new(
        encode_frame(301, 1, &vec![7; 2 * 1024 * 1024], CodecLimits::PRODUCTION)
            .expect("large canonical frame"),
    )
    .expect("large exact frame");
    let event = EventDraft::new(
        aggregate,
        EventSequence::first(),
        event(11),
        None,
        frame,
        Sha256Digest::new([11; 32]),
        Vec::new(),
    )
    .expect("large event draft");
    let request = AppendRequest::new(
        store_id(),
        command(11),
        Sha256Digest::new([12; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("large append plan");
    let error = journal.append(request).expect_err("page ceiling rejects growth");
    assert!(error.is_storage_exhausted());
    assert!(journal.head(aggregate).expect("head remains readable").is_none());
    assert_eq!(journal.integrity_scan().expect("journal remains valid").event_count(), 0);
}
