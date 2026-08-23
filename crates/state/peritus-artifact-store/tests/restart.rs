//! Restart recovery and injected crash-window tests.

mod support;

use std::fs;

use peritus_artifact_store::{ArtifactStore, StoreConfig};

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
