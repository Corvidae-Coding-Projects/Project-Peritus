use super::*;

fn confirmed(image: &ValidatedImage, preview: &[u8]) -> ControlOperation {
    let original = attach(image);
    let ControlIntent::AttachImage { image, text } = original.intent() else {
        panic!("image operation")
    };
    operation(
        2,
        1,
        ControlIntent::AttachImage {
            image: image.clone().with_preview_digest(sha256(preview)),
            text: text.clone(),
        },
    )
}

#[test]
fn exact_preview_and_image_publish_together_and_replay_after_restart() {
    let root = tempfile::tempdir().expect("root");
    let mut store = open_store(root.path());
    store.accept(&create()).expect("create");
    let image = image(false);
    let preview = b"exact provider-bound preview";
    let import = confirmed(&image, preview);
    assert!(store.accept_image(&import, &image).is_err(), "consent proof is required");
    assert!(store.accept_previewed_image(&import, &image, b"changed".to_vec()).is_err());
    assert!(store.accept_previewed_image(&attach(&image), &image, preview.to_vec()).is_err());
    assert!(store.resolve(&import).expect("absence").is_none());
    for namespace in [3407, 3408] {
        assert!(
            store.journal.state_record(namespace, import.id().as_bytes()).expect("row").is_none()
        );
    }
    let receipt = store.accept_previewed_image(&import, &image, preview.to_vec()).expect("import");
    let image_row =
        store.journal.state_record(3407, import.id().as_bytes()).expect("row").expect("image");
    let preview_row =
        store.journal.state_record(3408, import.id().as_bytes()).expect("row").expect("preview");
    assert_eq!(image_row.producing_position(), preview_row.producing_position());
    assert_eq!(preview_row.bytes(), preview);
    drop(store);
    let mut store = open_store(root.path());
    assert_eq!(store.resolve(&import).expect("resolve"), Some(receipt.clone()));
    assert_eq!(
        store.accept_previewed_image(&import, &image, preview.to_vec()).expect("replay"),
        receipt
    );
    assert_eq!(capture(&store).images(), std::slice::from_ref(image.media()));
}

#[test]
fn missing_changed_or_separately_published_consent_fails_closed() {
    for mutation in [
        "DELETE FROM state_records WHERE namespace = 3408",
        "UPDATE state_records SET producing_position = 1 WHERE namespace = 3408",
        "UPDATE state_records SET revision = 2 WHERE namespace = 3408",
        "UPDATE state_records SET value = x'00' WHERE namespace = 3408",
    ] {
        let root = tempfile::tempdir().expect("root");
        let mut store = open_store(root.path());
        store.accept(&create()).expect("create");
        let image = image(false);
        let preview = b"confirmed provider model and image";
        let import = confirmed(&image, preview);
        store.accept_previewed_image(&import, &image, preview.to_vec()).expect("import");
        drop(store);
        let connection =
            rusqlite::Connection::open(root.path().join("control.sqlite3")).expect("fixture");
        assert_eq!(connection.execute(mutation, []).expect("tamper"), 1);
        drop(connection);
        let store = open_store(root.path());
        assert!(store.load(create().conversation()).is_err());
        assert!(store.resolve(&import).is_err());
    }
}

#[test]
fn failed_preview_publication_leaves_no_image_input_or_receipt() {
    let root = tempfile::tempdir().expect("root");
    let mut store = open_store(root.path());
    store.accept(&create()).expect("create");
    let pages = store.journal.storage_pages().expect("pages");
    store.journal.limit_storage_pages(pages.page_count()).expect("freeze pages");
    let image = image(false);
    let preview = vec![42; 16 * 1024];
    let import = confirmed(&image, &preview);
    assert!(store.accept_previewed_image(&import, &image, preview).is_err());
    assert!(store.resolve(&import).expect("absence").is_none());
    assert_eq!(store.load(create().conversation()).expect("load").expect("root").revision(), 1);
    assert!(capture(&store).images().is_empty());
    for namespace in [3407, 3408] {
        assert!(
            store.journal.state_record(namespace, import.id().as_bytes()).expect("row").is_none()
        );
    }
}
