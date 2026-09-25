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
fn evaluation_reservation_is_stable_and_freezes_evidence() {
    let temp = tempfile::tempdir().expect("state");
    let path = temp.path().join("inbox.sqlite3");
    let mut store = Store::open(&path).expect("open");
    store.collect(workspace(1), run(2), "Improve validation", "observation").expect("collect");
    let id = store.inbox(workspace(1)).expect("read").candidates()[0].id().into_bytes();
    let original = Evaluation { run: [3; 16], target: [4; 16], providers: [[5; 16]; 3] };
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
                Evaluation { run: [7; 16], target: [4; 16], providers: [[5; 16]; 3] }
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
                Evaluation { run: [4; 16], target: [1; 16], providers: [[5; 16]; 3] }
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
    store.0.execute("UPDATE improvements SET record='{}'", []).expect("corrupt");
    assert!(store.inbox(workspace(1)).is_err());
    store.0.execute_batch("PRAGMA user_version=99").expect("future schema");
    drop(store);
    assert!(Store::open(&path).is_err());
}

#[test]
fn dismissed_suggestions_release_capacity_and_remain_deduplicated() {
    let temp = tempfile::tempdir().expect("state");
    let mut store = Store::open(&temp.path().join("inbox.sqlite3")).expect("open");
    for i in 0..MAX_IMPROVEMENTS {
        store
            .collect(workspace(1), run(2), &format!("Suggestion {i}"), "Evidence")
            .expect("collect");
    }
    assert!(store.collect(workspace(1), run(2), "Another suggestion", "Evidence").is_err());
    let id = store.inbox(workspace(1)).expect("inbox").candidates()[0].id().into_bytes();
    let proposal = store.get(workspace(1), id).expect("read").expect("candidate").proposal;
    store.dismiss(workspace(1), id).expect("dismiss");
    store
        .collect(workspace(1), run(2), "Another suggestion", "Evidence")
        .expect("room after dismissal");
    store.collect(workspace(1), run(3), &proposal, "More evidence").expect("retained tombstone");
    assert!(store.get(workspace(1), id).expect("read").expect("candidate").dismissed);
}

#[test]
fn changing_retained_evidence_without_its_digest_is_rejected() {
    let temp = tempfile::tempdir().expect("state");
    let mut store = Store::open(&temp.path().join("inbox.sqlite3")).expect("open");
    store.collect(workspace(1), run(2), "Suggestion", "Actual observation").expect("collect");
    store.0.execute("UPDATE improvements SET record=replace(record, 'Actual observation', 'Forged observation')", []).expect("alter");
    assert!(store.inbox(workspace(1)).is_err());
}
