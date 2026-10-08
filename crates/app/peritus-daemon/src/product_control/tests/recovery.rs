//! Transaction rollback and committed-corruption recovery boundaries.

use super::*;

#[test]
fn rolled_back_tail_is_ignored_but_committed_state_corruption_is_rejected() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    let create = create();
    let receipt = journal.accept(&create).expect("create");
    drop(journal);

    let mut connection =
        rusqlite::Connection::open(root.path().join("control.sqlite3")).expect("fault connection");
    {
        let transaction = connection.transaction().expect("uncommitted tail");
        transaction
            .execute(
                "UPDATE state_records SET value = zeroblob(length(value)) WHERE namespace = ?1",
                [i64::from(ROOT_NAMESPACE)],
            )
            .expect("stage torn state");
    }
    drop(connection);
    let journal = store(root.path());
    assert_eq!(journal.resolve(&create).expect("receipt after rollback"), Some(receipt));
    assert_eq!(
        journal.load(create.conversation()).expect("state after rollback").expect("record").title(),
        "Original title"
    );
    drop(journal);

    let connection = rusqlite::Connection::open(root.path().join("control.sqlite3"))
        .expect("corruption connection");
    connection
        .execute(
            "UPDATE state_records SET value = zeroblob(length(value)) WHERE namespace = ?1",
            [i64::from(ROOT_NAMESPACE)],
        )
        .expect("commit state corruption");
    drop(connection);
    let journal = store(root.path());
    assert!(matches!(
        journal.load(create.conversation()),
        Err(Error::Journal(ref error))
            if error.kind() == peritus_journal::JournalErrorKind::CorruptJournal
    ));
}
