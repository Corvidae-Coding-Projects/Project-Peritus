use super::*;

fn workspace(n: u8) -> WorkspaceId {
    WorkspaceId::new([n; 16]).expect("workspace")
}
fn run(n: u8) -> RunId {
    RunId::new([n; 16]).expect("run")
}

#[test]
fn collection_deduplicates_and_survives_restart_without_evaluation() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    {
        let mut store = Store::open(&path).expect("open");
        store
            .collect(workspace(1), run(2), "Improve validation", "a rejected tool call")
            .expect("collect");
        store
            .collect(workspace(1), run(2), " improve  VALIDATION ", "changed later")
            .expect("deduplicate");
        store
            .collect(workspace(1), run(3), "Improve validation", "a second observation")
            .expect("collect second");
        assert!(store.inbox(workspace(4)).expect("other workspace").candidates().is_empty());
    }
    let store = Store::open(&path).expect("restart");
    let page = store.inbox(workspace(1)).expect("read");
    assert_eq!(page.candidates().len(), 1);
    let item = &page.candidates()[0];
    assert_eq!(item.evidence().len(), 2);
    assert_eq!(item.evidence()[0].summary().as_str(), "a rejected tool call");
    assert_eq!(item.evaluation(), None);
}

#[test]
fn proposal_beyond_the_former_text_ceiling_survives_restart() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    let proposal = "Non-ASCII evidence é ".repeat(256);
    assert!(proposal.len() > 4096);
    {
        let mut store = Store::open(&path).expect("open");
        store.collect(workspace(1), run(2), &proposal, "Observation").expect("collect");
    }
    let store = Store::open(&path).expect("restart");
    assert_eq!(
        store.inbox(workspace(1)).expect("read").candidates()[0].proposal().as_str(),
        proposal
    );
}

#[test]
fn evaluation_reservation_is_stable_and_freezes_evidence() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    let mut store = Store::open(&path).expect("open");
    store.collect(workspace(1), run(2), "Improve validation", "observation").expect("collect");
    let id = store.inbox(workspace(1)).expect("read").candidates()[0].id().into_bytes();
    let actor = ActorId::new([9; 16]).expect("actor");
    let conversation =
        super::super::evaluation::derived_conversation(actor, workspace(1), id, run(3))
            .expect("conversation");
    let original = Evaluation {
        actor: actor.into_bytes(),
        conversation: conversation.into_bytes(),
        run: [3; 16],
        target: [4; 16],
        providers: [[5; 16]; 3],
    };
    store.reserve(workspace(1), id, original.clone()).expect("reserve");
    store.collect(workspace(1), run(6), "Improve validation", "later evidence").expect("frozen");
    drop(store);
    let mut store = Store::open(&path).expect("restart");
    let again = store
        .reserve(workspace(1), id, Evaluation { run: [7; 16], ..original })
        .expect("same evaluation");
    assert_eq!(again.evaluation.expect("evaluation").run, original.run);
    assert_eq!(again.evidence.len(), 1);
    assert!(store.reserve(workspace(1), id, Evaluation { target: [8; 16], ..original }).is_err());
    assert!(
        store
            .reserve(
                workspace(9),
                id,
                Evaluation {
                    actor: [9; 16],
                    conversation: [10; 16],
                    run: [7; 16],
                    target: [4; 16],
                    providers: [[5; 16]; 3],
                }
            )
            .is_err()
    );
}

#[test]
fn dismissal_is_durable_and_cannot_be_auto_reactivated() {
    let temp = tempfile::tempdir().expect("state");
    let mut store = Store::open(&temp.path().join("inbox.sqlite3")).expect("open");
    store.collect(workspace(1), run(2), "Suggestion", "Observation").expect("collect");
    let id = store.inbox(workspace(1)).expect("read").candidates()[0].id().into_bytes();
    store.dismiss(workspace(1), id).expect("dismiss");
    store.collect(workspace(1), run(3), "Suggestion", "Another observation").expect("collect");
    assert!(store.inbox(workspace(1)).expect("read").candidates()[0].dismissed());
    assert!(
        store
            .reserve(
                workspace(1),
                id,
                Evaluation {
                    actor: [9; 16],
                    conversation: super::super::evaluation::derived_conversation(
                        ActorId::new([9; 16]).expect("actor"),
                        workspace(1),
                        id,
                        run(4),
                    )
                    .expect("conversation")
                    .into_bytes(),
                    run: [4; 16],
                    target: [1; 16],
                    providers: [[5; 16]; 3],
                }
            )
            .is_err()
    );
}

#[test]
fn corrupt_and_future_records_are_not_treated_as_empty_inboxes() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    let mut store = Store::open(&path).expect("open");
    store.collect(workspace(1), run(2), "Suggestion", "Observation").expect("collect");
    store.0.execute("UPDATE improvement_candidates SET proposal='corrupt'", []).expect("corrupt");
    assert!(store.inbox(workspace(1)).is_err());
    store.0.execute_batch("PRAGMA user_version=99").expect("future schema");
    drop(store);
    assert!(Store::open(&path).is_err());
}

#[test]
fn pre_release_schema_is_quarantined_without_migration() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    let connection = Connection::open(&path).expect("open fixture");
    connection
        .execute_batch(
            "CREATE TABLE improvements (workspace BLOB NOT NULL, id BLOB NOT NULL, record TEXT NOT NULL, PRIMARY KEY(workspace,id)); INSERT INTO improvements VALUES (x'01', x'02', 'legacy'); PRAGMA user_version=1;",
        )
        .expect("old schema");
    drop(connection);
    let store = Store::open(&path).expect("clean cut opens current store");
    assert!(store.inbox(workspace(1)).expect("empty current inbox").candidates().is_empty());
    drop(store);
    let current = Connection::open(&path).expect("current store");
    let current_version: u32 =
        current.pragma_query_value(None, "user_version", |row| row.get(0)).expect("version");
    assert_eq!(current_version, CURRENT_SCHEMA);
    assert_eq!(
        current
            .query_row("SELECT count(*) FROM improvement_candidates", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        0
    );
    let retained = temp.path().join("improvements-quarantine/inbox.sqlite3.schema-1");
    let legacy = Connection::open_with_flags(&retained, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("retained legacy store");
    let legacy_version: u32 =
        legacy.pragma_query_value(None, "user_version", |row| row.get(0)).expect("version");
    assert_eq!(legacy_version, PRE_RELEASE_SCHEMA);
    assert_eq!(
        legacy
            .query_row("SELECT record FROM improvements", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "legacy"
    );
}

#[test]
fn suggestions_grow_past_the_old_ceiling_and_dismissal_remains_deduplicated() {
    let temp = tempfile::tempdir().expect("state");
    let mut store = Store::open(&temp.path().join("inbox.sqlite3")).expect("open");
    for i in 0..40 {
        store
            .collect(workspace(1), run(2), &format!("Suggestion {i}"), "Evidence")
            .expect("collect");
    }
    assert_eq!(store.inbox(workspace(1)).expect("inbox").candidates().len(), 40);
    let id = store.inbox(workspace(1)).expect("inbox").candidates()[0].id().into_bytes();
    let proposal = store.get(workspace(1), id).expect("read").expect("candidate").proposal;
    store.dismiss(workspace(1), id).expect("dismiss");
    store
        .collect(workspace(1), run(2), "Another suggestion", "Evidence")
        .expect("collection after dismissal");
    store.collect(workspace(1), run(3), &proposal, "More evidence").expect("retained tombstone");
    assert!(store.get(workspace(1), id).expect("read").expect("candidate").dismissed);
    assert_eq!(store.inbox(workspace(1)).expect("inbox").candidates().len(), 41);
}

#[test]
fn suggestion_evidence_grows_past_four_runs_and_survives_restart() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    let mut store = Store::open(&path).expect("open");
    for index in 1..=8 {
        store.collect(workspace(1), run(index), "Suggestion", "Observation").expect("collect");
    }
    drop(store);

    let store = Store::open(&path).expect("restart");
    let inbox = store.inbox(workspace(1)).expect("read");
    assert_eq!(inbox.candidates()[0].evidence().len(), 8);
    assert_eq!(inbox.candidates()[0].evidence()[0].run(), run(1));
    assert_eq!(inbox.candidates()[0].evidence()[7].run(), run(8));
}

#[test]
fn changing_retained_evidence_without_its_digest_is_rejected() {
    let temp = tempfile::tempdir().expect("state");
    let mut store = Store::open(&temp.path().join("inbox.sqlite3")).expect("open");
    store.collect(workspace(1), run(2), "Suggestion", "Actual observation").expect("collect");
    store.0.execute("UPDATE improvement_evidence SET summary='Forged observation'", []).expect("alter");
    assert!(store.inbox(workspace(1)).is_err());
}
