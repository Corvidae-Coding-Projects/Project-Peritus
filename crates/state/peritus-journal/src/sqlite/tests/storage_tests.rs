use std::{
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use peritus_codec::{CodecLimits, encode_frame};
use peritus_types::{EventSequence, Sha256Digest};
use rusqlite::params;
use tempfile::TempDir;

use crate::{
    AggregateKind, AppendRequest, CommandResolution, EventDraft, ExactFrame, HeadExpectation,
    JournalCancellation, JournalErrorKind, SqliteJournal, SqliteJournalOptions,
};

use super::{command, draft, event, key, open, plan, store_id};

#[test]
fn version_one_artifact_catalog_migrates_without_losing_identity_or_metadata() {
    use crate::NewApplicationArtifact;
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
        ([2_u8; 16], 1, JournalErrorKind::InvalidInput),
        ([1_u8; 16], 99, JournalErrorKind::UnsupportedSchema),
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
        let error = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())
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

#[test]
fn default_open_waits_past_five_seconds_then_reopens_the_exact_committed_receipt() {
    let temp = TempDir::new().expect("temporary directory");
    let path = temp.path().join("journal.sqlite3");
    let aggregate = key(AggregateKind::Kernel, 70);
    let command_id = command(70);
    let request_digest = Sha256Digest::new([70; 32]);
    let mut initial = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())
        .expect("initial journal");
    let committed = initial
        .append(plan(
            command_id,
            request_digest,
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(70), None, 70)],
        ))
        .expect("commit receipt before contention");
    let expected_hash = committed.batch_hash();
    let expected_frame = committed.records()[0].frame_bytes().to_vec();
    drop(initial);

    let blocker = rusqlite::Connection::open(&path).expect("contention connection");
    blocker.execute_batch("BEGIN IMMEDIATE;").expect("hold writer ownership");
    let started = Instant::now();
    let opener = thread::spawn(move || {
        SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())
    });
    thread::sleep(Duration::from_millis(5_100));
    assert!(!opener.is_finished(), "the former five-second cutoff must not end the open");
    blocker.execute_batch("ROLLBACK;").expect("release writer ownership");
    let reopened = opener.join().expect("join opener").expect("open after ownership release");
    assert!(started.elapsed() >= Duration::from_secs(5));

    let CommandResolution::Committed(resolved) = reopened
        .resolve_command(command_id, request_digest)
        .expect("resolve committed command after reopen")
    else {
        panic!("committed command must retain its exact receipt");
    };
    assert_eq!(resolved.batch_hash(), expected_hash);
    assert_eq!(resolved.records()[0].frame_bytes(), expected_frame);
    assert_eq!((resolved.first_position(), resolved.last_position()), (1, 1));
}

#[test]
fn caller_cancellation_ends_a_blocked_open_without_replacing_durable_identity() {
    let temp = TempDir::new().expect("temporary directory");
    let path = temp.path().join("journal.sqlite3");
    let initial = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())
        .expect("install journal");
    drop(initial);
    let blocker = rusqlite::Connection::open(&path).expect("contention connection");
    blocker.execute_batch("BEGIN IMMEDIATE;").expect("hold writer ownership");

    let cancellation = JournalCancellation::new();
    let waiting = Arc::new(Barrier::new(2));
    let opener_waiting = Arc::clone(&waiting);
    let opener_cancellation = cancellation.clone();
    let opener = thread::spawn(move || {
        opener_waiting.wait();
        SqliteJournal::open_waiting(&path, store_id(), &opener_cancellation)
    });
    waiting.wait();
    cancellation.cancel();
    let error = opener
        .join()
        .expect("join cancelled opener")
        .err()
        .expect("cancelled contention must not acquire the journal");
    assert_eq!(error.kind(), JournalErrorKind::Busy);

    blocker.execute_batch("ROLLBACK;").expect("release writer ownership");
    let reopened = SqliteJournal::open(
        temp.path().join("journal.sqlite3"),
        store_id(),
        SqliteJournalOptions::default(),
    )
    .expect("same durable identity reopens after cancellation");
    assert_eq!(reopened.store_id(), store_id());
}
