use super::route::{
    BLOB_AFTER_BEFORE_ACK, BLOB_BEFORE, CommitRoute, GATE_AFTER_BEFORE_ACK, GATE_BEFORE,
    JOURNAL_AFTER_BEFORE_ACK, JOURNAL_BEFORE, LEASE_AFTER_BEFORE_ACK, LEASE_BEFORE,
    PATCH_AFTER_BEFORE_ACK, PATCH_BEFORE, PROMOTION_AFTER_BEFORE_ACK, PROMOTION_BEFORE,
    SNAPSHOT_AFTER_BEFORE_ACK, SNAPSHOT_BEFORE,
};
use super::{FaultDocument, ScenarioDocument};

#[test]
fn only_the_fourteen_real_commit_routes_are_admitted() {
    let cases = [
        (JOURNAL_BEFORE, "journal", CommitRoute::JournalBeforeDurableCommit),
        (BLOB_BEFORE, "blob", CommitRoute::BlobBeforeDurableCommit),
        (SNAPSHOT_BEFORE, "snapshot", CommitRoute::SnapshotBeforeDurableCommit),
        (LEASE_BEFORE, "lease", CommitRoute::LeaseBeforeDurableCommit),
        (PATCH_BEFORE, "patch", CommitRoute::PatchBeforeDurableCommit),
        (GATE_BEFORE, "gate", CommitRoute::GateBeforeDurableCommit),
        (PROMOTION_BEFORE, "promotion", CommitRoute::PromotionBeforeDurableCommit),
    ];
    for (id, boundary, expected) in cases {
        let value = scenario(id, boundary, "before-durable-commit", "rolled-back-uncommitted");
        assert_eq!(CommitRoute::from_scenario(&value), Some(expected));
    }
    let after_cases = [
        (JOURNAL_AFTER_BEFORE_ACK, "journal", CommitRoute::JournalAfterDurableCommitBeforeAck),
        (BLOB_AFTER_BEFORE_ACK, "blob", CommitRoute::BlobAfterDurableCommitBeforeAck),
        (SNAPSHOT_AFTER_BEFORE_ACK, "snapshot", CommitRoute::SnapshotAfterDurableCommitBeforeAck),
        (LEASE_AFTER_BEFORE_ACK, "lease", CommitRoute::LeaseAfterDurableCommitBeforeAck),
        (PATCH_AFTER_BEFORE_ACK, "patch", CommitRoute::PatchAfterDurableCommitBeforeAck),
        (GATE_AFTER_BEFORE_ACK, "gate", CommitRoute::GateAfterDurableCommitBeforeAck),
        (
            PROMOTION_AFTER_BEFORE_ACK,
            "promotion",
            CommitRoute::PromotionAfterDurableCommitBeforeAck,
        ),
    ];
    for (id, boundary, expected) in after_cases {
        let value = scenario(id, boundary, "after-durable-commit-before-ack", "replayed-committed");
        assert_eq!(CommitRoute::from_scenario(&value), Some(expected));
    }
    let unsupported = scenario(
        "h1.crash.unknown.before",
        "unknown",
        "before-durable-commit",
        "rolled-back-uncommitted",
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
