//! Exclusive owner and `SQLite` writer contention behavior.

use super::*;
use peritus_product_runner::control::{ControlIntent, ControlText};
use std::time::Duration;

#[test]
fn control_store_has_one_owner_and_rejects_wrong_store_identity_after_release() {
    let root = tempfile::tempdir().expect("root");
    let first = store(root.path());
    let cancellation = JournalCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        ControlStore::open_cancellable(
            root.path(),
            StoreId::new([1; 16]).expect("store"),
            &cancellation,
        ),
        Err(Error::ContentionCancelled)
    ));
    drop(first);
    assert!(ControlStore::open(root.path(), StoreId::new([5; 16]).expect("wrong store")).is_err());
}

#[test]
fn control_store_owner_waits_beyond_the_former_deadline_then_opens_after_release() {
    let root = tempfile::tempdir().expect("root");
    let first = store(root.path());
    let path = root.path().to_owned();
    let (completed, completion) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        completed
            .send(ControlStore::open(&path, StoreId::new([1; 16]).expect("store")).map(|_| ()))
            .expect("report owner acquisition");
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(350)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    drop(first);
    completion
        .recv_timeout(Duration::from_secs(5))
        .expect("open resumes after owner release")
        .expect("reopen succeeds");
    worker.join().expect("owner waiter");
}

#[test]
#[allow(
    clippy::used_underscore_binding,
    reason = "the regression duplicates the otherwise unread RAII ownership guard to model descriptor inheritance"
)]
fn closing_the_store_releases_ownership_even_while_a_duplicated_handle_exists() {
    let root = tempfile::tempdir().expect("root");
    let journal = store(root.path());
    // A concurrent subprocess fork can temporarily inherit this open file description,
    // even though close-on-exec prevents it from surviving the eventual exec.
    let inherited = journal._owner.0.try_clone().expect("duplicate owner handle");
    drop(journal);
    let reopened = store(root.path());
    assert!(reopened.load(create().conversation()).expect("empty root").is_none());
    drop(inherited);
}

#[test]
fn sqlite_writer_contention_outlives_the_former_deadline_then_commits_and_reopens_exactly() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    let rename = operation(
        2,
        1,
        ControlIntent::RenameConversation {
            title: ControlText::new("Committed after contention".to_owned()).expect("title"),
        },
    );
    let blocker = rusqlite::Connection::open(root.path().join("control.sqlite3"))
        .expect("contention connection");
    blocker.execute_batch("BEGIN IMMEDIATE;").expect("hold SQLite writer ownership");
    let (completed, completion) = std::sync::mpsc::sync_channel(1);
    let operation = rename.clone();
    let worker = std::thread::spawn(move || {
        completed.send(journal.accept(&operation)).expect("report append result");
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(350)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    blocker.execute_batch("ROLLBACK;").expect("release SQLite writer ownership");
    let receipt = completion
        .recv_timeout(Duration::from_secs(5))
        .expect("append resumes after ownership release")
        .expect("append succeeds");
    worker.join().expect("contention worker");

    let journal = store(root.path());
    assert_eq!(journal.resolve(&rename).expect("resolve after reopen"), Some(receipt));
    assert_eq!(
        journal.load(rename.conversation()).expect("load").expect("record").title(),
        "Committed after contention"
    );
}

#[test]
fn cancelled_contention_keeps_the_original_command_retryable() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    let rename = operation(
        2,
        1,
        ControlIntent::RenameConversation {
            title: ControlText::new("Original retry identity".to_owned()).expect("title"),
        },
    );
    let blocker = rusqlite::Connection::open(root.path().join("control.sqlite3"))
        .expect("contention connection");
    blocker.execute_batch("BEGIN IMMEDIATE;").expect("hold SQLite writer ownership");
    let cancellation = JournalCancellation::new();
    let worker_cancellation = cancellation.clone();
    let operation = rename.clone();
    let (completed, completion) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let result = worker_cancellation.run(|| journal.accept(&operation));
        completed.send(result).expect("report cancelled append");
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(350)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    cancellation.cancel();
    let error = completion
        .recv_timeout(Duration::from_secs(5))
        .expect("cancellation interrupts contention")
        .expect_err("cancelled append is not acknowledged");
    assert!(matches!(error, Error::Journal(ref error) if error.is_contention()));
    worker.join().expect("contention worker");
    blocker.execute_batch("ROLLBACK;").expect("release SQLite writer ownership");

    let mut journal = store(root.path());
    assert!(journal.resolve(&rename).expect("cancelled command absent").is_none());
    let receipt = journal.accept(&rename).expect("retry original command");
    drop(journal);
    let journal = store(root.path());
    assert_eq!(journal.resolve(&rename).expect("exact receipt after reopen"), Some(receipt));
}
