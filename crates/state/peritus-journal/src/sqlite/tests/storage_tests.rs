use peritus_codec::{CodecLimits, encode_frame};
use peritus_types::{EventSequence, Sha256Digest};
use rusqlite::params;
use tempfile::TempDir;

use crate::{AggregateKind, AppendRequest, EventDraft, ExactFrame, HeadExpectation};

use super::{command, event, key, open, store_id};

#[test]
fn version_one_artifact_catalog_migrates_without_losing_identity_or_metadata() {
    use crate::{NewApplicationArtifact, SqliteJournal, SqliteJournalOptions};
    use peritus_types::ArtifactId;

    let temp = TempDir::new().expect("temporary directory");
    let path = temp.path().join("old.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("old database");
    let schema = super::super::schema::INSTALL_SCHEMA.replace(
        "digest BLOB NOT NULL CHECK (length(digest) = 32)",
        "digest BLOB NOT NULL UNIQUE CHECK (length(digest) = 32)",
    );
    connection.execute_batch(&schema).expect("version one tables");
    connection
        .execute("INSERT INTO store_meta VALUES (1, ?1, 1)", [store_id().as_bytes().as_slice()])
        .expect("old store binding");
    connection.pragma_update(None, "user_version", 1).expect("old version");
    connection
        .execute(
            "INSERT INTO app_artifacts VALUES (?1, ?2, 12, 'text/plain', 1, NULL)",
            params![[3_u8; 16], [4_u8; 32]],
        )
        .expect("original attachment");
    drop(connection);

    let mut journal =
        SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default()).expect("upgrade");
    let original =
        journal.application_artifact(ArtifactId::new([3; 16]).unwrap()).unwrap().unwrap();
    let duplicate = NewApplicationArtifact::new(
        ArtifactId::new([5; 16]).unwrap(),
        original.digest(),
        original.byte_size(),
        original.media_type().to_owned(),
    )
    .unwrap();
    journal.begin_application_artifact(duplicate).expect("distinct identity, same content");
    drop(journal);
    let journal = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())
        .expect("repeat open");
    assert_eq!(journal.application_artifact(original.artifact_id()).unwrap(), Some(original));
    assert!(journal.application_artifact(ArtifactId::new([5; 16]).unwrap()).unwrap().is_some());
}

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
fn version_two_history_upgrade_preserves_nodes_and_removes_capacity_checks() {
    let temp = TempDir::new().expect("temporary directory");
    let path = temp.path().join("old-history.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("old database");
    let schema = super::super::schema::INSTALL_SCHEMA
        .replace("level BETWEEN 0 AND 255", "level BETWEEN 0 AND 4")
        .replace("byte_length >= 0", "byte_length BETWEEN 0 AND 16777216");
    connection.execute_batch(&schema).expect("version two tables");
    connection
        .execute("INSERT INTO store_meta VALUES (1, ?1, 2)", [store_id().as_bytes().as_slice()])
        .expect("old store binding");
    connection.pragma_update(None, "user_version", 2).expect("old version");
    connection
        .execute(
            "INSERT INTO state_history_nodes VALUES (?1, 0, 1, ?2)",
            params![[3_u8; 32], [7_u8]],
        )
        .expect("retained history node");
    drop(connection);
    let journal =
        crate::SqliteJournal::open(&path, store_id(), crate::SqliteJournalOptions::default())
            .expect("history upgrade");
    let retained: (i64, i64, Vec<u8>) = journal
        .connection
        .query_row(
            "SELECT level, byte_length, payload FROM state_history_nodes WHERE node_digest = ?1",
            [[3_u8; 32]],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("retained node");
    assert_eq!(retained, (0, 1, vec![7]));
    journal
        .connection
        .execute(
            "INSERT INTO state_history_nodes VALUES (?1, 5, 33554433, ?2)",
            params![[4_u8; 32], [8_u8; 64]],
        )
        .expect("larger logical history node admitted");
    assert_eq!(
        journal
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .expect("new version"),
        3
    );
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
