use super::{
    BLOB_AFTER_BEFORE_ACK, BLOB_BEFORE, CommitRoute, FaultDocument, JOURNAL_AFTER_BEFORE_ACK,
    JOURNAL_BEFORE, LEASE_AFTER_BEFORE_ACK, LEASE_BEFORE, PATCH_AFTER_BEFORE_ACK, PATCH_BEFORE,
    SNAPSHOT_AFTER_BEFORE_ACK, SNAPSHOT_BEFORE, ScenarioDocument,
};

#[test]
fn only_the_ten_real_commit_routes_are_admitted() {
    let before =
        scenario(JOURNAL_BEFORE, "journal", "before-durable-commit", "rolled-back-uncommitted");
    let after = scenario(
        JOURNAL_AFTER_BEFORE_ACK,
        "journal",
        "after-durable-commit-before-ack",
        "replayed-committed",
    );
    let snapshot_before =
        scenario(SNAPSHOT_BEFORE, "snapshot", "before-durable-commit", "rolled-back-uncommitted");
    let snapshot_after = scenario(
        SNAPSHOT_AFTER_BEFORE_ACK,
        "snapshot",
        "after-durable-commit-before-ack",
        "replayed-committed",
    );
    let lease_before =
        scenario(LEASE_BEFORE, "lease", "before-durable-commit", "rolled-back-uncommitted");
    let lease_after = scenario(
        LEASE_AFTER_BEFORE_ACK,
        "lease",
        "after-durable-commit-before-ack",
        "replayed-committed",
    );
    let patch_before =
        scenario(PATCH_BEFORE, "patch", "before-durable-commit", "rolled-back-uncommitted");
    let patch_after = scenario(
        PATCH_AFTER_BEFORE_ACK,
        "patch",
        "after-durable-commit-before-ack",
        "replayed-committed",
    );
    let unsupported = scenario(
        "h1.crash.gate.before",
        "gate",
        "before-durable-commit",
        "rolled-back-uncommitted",
    );
    let blob_before =
        scenario(BLOB_BEFORE, "blob", "before-durable-commit", "rolled-back-uncommitted");
    let blob_after = scenario(
        BLOB_AFTER_BEFORE_ACK,
        "blob",
        "after-durable-commit-before-ack",
        "replayed-committed",
    );
    assert_eq!(CommitRoute::from_scenario(&before), Some(CommitRoute::JournalBeforeDurableCommit));
    assert_eq!(
        CommitRoute::from_scenario(&after),
        Some(CommitRoute::JournalAfterDurableCommitBeforeAck)
    );
    assert_eq!(
        CommitRoute::from_scenario(&blob_before),
        Some(CommitRoute::BlobBeforeDurableCommit)
    );
    assert_eq!(
        CommitRoute::from_scenario(&blob_after),
        Some(CommitRoute::BlobAfterDurableCommitBeforeAck)
    );
    assert_eq!(
        CommitRoute::from_scenario(&lease_before),
        Some(CommitRoute::LeaseBeforeDurableCommit)
    );
    assert_eq!(
        CommitRoute::from_scenario(&lease_after),
        Some(CommitRoute::LeaseAfterDurableCommitBeforeAck)
    );
    assert_eq!(
        CommitRoute::from_scenario(&patch_before),
        Some(CommitRoute::PatchBeforeDurableCommit)
    );
    assert_eq!(
        CommitRoute::from_scenario(&patch_after),
        Some(CommitRoute::PatchAfterDurableCommitBeforeAck)
    );
    assert_eq!(
        CommitRoute::from_scenario(&snapshot_before),
        Some(CommitRoute::SnapshotBeforeDurableCommit)
    );
    assert_eq!(
        CommitRoute::from_scenario(&snapshot_after),
        Some(CommitRoute::SnapshotAfterDurableCommitBeforeAck)
    );
    assert_eq!(CommitRoute::from_scenario(&unsupported), None);
}

fn scenario(id: &str, boundary: &str, timing: &str, expected_recovery: &str) -> ScenarioDocument {
    ScenarioDocument {
        id: id.to_owned(),
        title: "journal crash".to_owned(),
        fault: FaultDocument::CommitCrash {
            boundary: boundary.to_owned(),
            timing: timing.to_owned(),
        },
        expected_recovery: expected_recovery.to_owned(),
    }
}
