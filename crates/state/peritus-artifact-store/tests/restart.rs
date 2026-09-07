//! Restart recovery and injected crash-window tests.

mod support;

use std::fs;

use peritus_artifact_store::{
    ArtifactStore, ErrorCode, IntegrityState, ReferenceOwner, StoreConfig,
};

use support::{digest, object_path, quarantine_path, request, store};

#[test]
fn read_only_inspection_verifies_bounds_and_bytes_without_recovery() {
    let missing = tempfile::tempdir().unwrap();
    let absent = missing.path().join("absent");
    let config = StoreConfig::new(&absent, 128, 512).unwrap();
    assert!(ArtifactStore::read_existing(&config, digest(b"missing"), 128).is_err());
    assert!(!absent.exists());
    let (directory, store) = store(128, 512);
    let bytes = b"exact published bytes";
    let mut writer = store.begin_write(request(bytes, 64, 1)).unwrap();
    writer.write_chunk(bytes).unwrap();
    writer.finalize().unwrap();
    let temporary = directory.path().join("temporary").join("do-not-recover.tmp");
    fs::write(&temporary, b"pending write").unwrap();
    let config = StoreConfig::new(directory.path(), 128, 512).unwrap();
    assert_eq!(ArtifactStore::read_existing(&config, digest(bytes), 128).unwrap(), bytes);
    assert!(ArtifactStore::read_existing(&config, digest(bytes), 4).is_err());
    assert!(temporary.exists());
    let object = object_path(directory.path(), digest(bytes));
    fs::write(&object, b"corrupt").unwrap();
    assert!(ArtifactStore::read_existing(&config, digest(bytes), 128).is_err());
    assert!(object.exists(), "inspection must not quarantine corrupt bytes");
    assert!(temporary.exists());
}

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
fn fresh_catalog_includes_integrity_state_with_a_healthy_default() {
    let (directory, store) = store(128, 512);
    drop(store);
    let connection = rusqlite::Connection::open(directory.path().join("metadata.sqlite3"))
        .expect("initial catalog");
    let column: (bool, String) = connection.query_row(
        "SELECT [notnull], dflt_value FROM pragma_table_info('artifact_records') WHERE name = 'integrity_state'",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).expect("initial integrity column");
    assert_eq!(column, (true, "1".to_owned()));
}
