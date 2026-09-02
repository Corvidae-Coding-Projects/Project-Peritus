//! Restart recovery and injected crash-window tests.

mod support;

use std::fs;

use peritus_artifact_store::{
    ArtifactStore, ErrorCode, IntegrityState, ReferenceOwner, StoreConfig,
};

use support::{digest, object_path, quarantine_path, request, store};

#[test]
fn restart_removes_abandoned_partial_temporary_file() {
    let (directory, store) = store(128, 512);
    drop(store);
    let temporary = directory.path().join("temporary").join("abandoned.tmp");
    fs::write(&temporary, b"partial bytes").expect("inject abandoned temporary file");

    let reopened =
        ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
            .expect("restart recovery");
    assert!(!temporary.exists());
    drop(reopened);
}

#[test]
fn publication_before_catalog_insert_is_quarantined_then_swept_on_later_restart() {
    let (directory, store) = store(128, 512);
    drop(store);
    let bytes = b"published without metadata";
    let content_digest = digest(bytes);
    let object = object_path(directory.path(), content_digest);
    fs::create_dir_all(object.parent().expect("object parent")).expect("prefix");
    fs::write(&object, bytes).expect("inject post-publication crash window");

    let recovered_once =
        ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
            .expect("first recovery quarantines");
    assert!(!object.exists());
    assert!(quarantine_path(directory.path(), content_digest).exists());
    drop(recovered_once);

    let recovered_twice =
        ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
            .expect("later recovery sweeps orphan quarantine");
    assert!(!quarantine_path(directory.path(), content_digest).exists());
    drop(recovered_twice);
}

#[test]
fn durable_quarantine_state_completes_move_after_restart() {
    let (directory, store) = store(128, 512);
    let bytes = b"crash-window-object";
    let content_digest = digest(bytes);
    let mut writer = store.begin_write(request(bytes, 64, 1)).expect("writer");
    writer.write_chunk(bytes).expect("bytes");
    writer.finalize().expect("finalize");
    drop(store);

    let connection = rusqlite::Connection::open(directory.path().join("metadata.sqlite3"))
        .expect("open catalog for fault injection");
    connection
        .execute(
            "UPDATE artifact_records
            SET quarantine_state = 2, quarantine_generation = 1
          WHERE digest = ?1",
            [content_digest.as_bytes().as_slice()],
        )
        .expect("inject crash after durable state and before rename");
    drop(connection);

    let recovered =
        ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
            .expect("recovery completes move");
    assert!(!object_path(directory.path(), content_digest).exists());
    assert!(quarantine_path(directory.path(), content_digest).exists());
    drop(recovered);
}

#[test]
fn restart_contains_referenced_corrupt_object_without_losing_its_audit_root() {
    let (directory, store) = store(128, 512);
    let bytes = b"referenced-content";
    let content_digest = digest(bytes);
    let mut writer = store.begin_write(request(bytes, 64, 2)).expect("writer");
    writer.write_chunk(bytes).expect("bytes");
    writer.finalize().expect("finalize");
    let owner = ReferenceOwner::evidence(digest(b"restart-corruption-owner").sha256());
    store.add_reference(owner, content_digest).expect("reference");
    drop(store);

    let object = object_path(directory.path(), content_digest);
    fs::write(&object, b"divergent-content").expect("inject same-size corruption");

    let recovered =
        ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
            .expect("restart contains corruption");
    let metadata =
        recovered.metadata(content_digest).expect("metadata query").expect("metadata retained");
    assert_eq!(metadata.integrity(), IntegrityState::Corrupt);
    assert!(!metadata.is_referenceable());
    assert_eq!(
        recovered.verify(content_digest).expect_err("corrupt object is unavailable").code(),
        ErrorCode::MissingArtifact
    );
    assert!(recovered.reference_roots().expect("roots").contains(&content_digest));
    assert!(!object.exists());
    assert!(quarantine_path(directory.path(), content_digest).exists());
    drop(recovered);

    let reopened =
        ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
            .expect("contained corruption remains restart-safe");
    assert_eq!(
        reopened
            .metadata(content_digest)
            .expect("metadata query")
            .expect("metadata retained")
            .integrity(),
        IntegrityState::Corrupt
    );
}

#[test]
fn opening_a_pre_integrity_catalog_adds_the_healthy_default() {
    let directory = tempfile::tempdir().expect("temporary store root");
    let database = directory.path().join("metadata.sqlite3");
    let connection = rusqlite::Connection::open(&database).expect("legacy catalog");
    connection
        .execute_batch(
            "CREATE TABLE artifact_records (
                digest BLOB PRIMARY KEY NOT NULL CHECK(length(digest) = 32),
                size INTEGER NOT NULL CHECK(size >= 0),
                media_type TEXT NOT NULL,
                encryption_algorithm TEXT,
                encryption_key_reference BLOB,
                encryption_parameters_digest BLOB,
                finalization_state INTEGER NOT NULL CHECK(finalization_state IN (1, 2)),
                creating_event BLOB NOT NULL CHECK(length(creating_event) = 16),
                quarantine_state INTEGER NOT NULL CHECK(quarantine_state IN (1, 2)),
                quarantine_generation INTEGER
            ) STRICT;
            CREATE TABLE artifact_references (
                owner_kind INTEGER NOT NULL,
                owner_identity BLOB NOT NULL,
                artifact_digest BLOB NOT NULL,
                PRIMARY KEY(owner_kind, owner_identity, artifact_digest),
                FOREIGN KEY(artifact_digest) REFERENCES artifact_records(digest) ON DELETE RESTRICT
            ) STRICT;",
        )
        .expect("legacy schema");
    drop(connection);

    let store = ArtifactStore::open(StoreConfig::new(directory.path(), 128, 512).expect("config"))
        .expect("legacy catalog migrates");
    drop(store);
    let connection = rusqlite::Connection::open(database).expect("migrated catalog");
    let mut statement = connection.prepare("PRAGMA table_info(artifact_records)").expect("pragma");
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("column query")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");
    assert!(columns.iter().any(|column| column == "integrity_state"));
}
