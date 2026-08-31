use super::route::{
    BLOB_AFTER_BEFORE_ACK, BLOB_BEFORE, GATE_AFTER_BEFORE_ACK, GATE_BEFORE,
    JOURNAL_AFTER_BEFORE_ACK, JOURNAL_BEFORE, LEASE_AFTER_BEFORE_ACK, LEASE_BEFORE,
    PATCH_AFTER_BEFORE_ACK, PATCH_BEFORE, PROJECTION_CORRUPTION, PROMOTION_AFTER_BEFORE_ACK,
    PROMOTION_BEFORE, SNAPSHOT_AFTER_BEFORE_ACK, SNAPSHOT_BEFORE, ScenarioRoute,
};
use super::{FaultDocument, ScenarioDocument};

#[test]
fn only_the_fifteen_real_routes_are_admitted() {
    let cases = [
        (JOURNAL_BEFORE, "journal", ScenarioRoute::JournalBeforeDurableCommit),
        (BLOB_BEFORE, "blob", ScenarioRoute::BlobBeforeDurableCommit),
        (SNAPSHOT_BEFORE, "snapshot", ScenarioRoute::SnapshotBeforeDurableCommit),
        (LEASE_BEFORE, "lease", ScenarioRoute::LeaseBeforeDurableCommit),
        (PATCH_BEFORE, "patch", ScenarioRoute::PatchBeforeDurableCommit),
        (GATE_BEFORE, "gate", ScenarioRoute::GateBeforeDurableCommit),
        (PROMOTION_BEFORE, "promotion", ScenarioRoute::PromotionBeforeDurableCommit),
    ];
    for (id, boundary, expected) in cases {
        let value = scenario(id, boundary, "before-durable-commit", "rolled-back-uncommitted");
        assert_eq!(ScenarioRoute::from_scenario(&value), Some(expected));
    }
    let after_cases = [
        (JOURNAL_AFTER_BEFORE_ACK, "journal", ScenarioRoute::JournalAfterDurableCommitBeforeAck),
        (BLOB_AFTER_BEFORE_ACK, "blob", ScenarioRoute::BlobAfterDurableCommitBeforeAck),
        (SNAPSHOT_AFTER_BEFORE_ACK, "snapshot", ScenarioRoute::SnapshotAfterDurableCommitBeforeAck),
        (LEASE_AFTER_BEFORE_ACK, "lease", ScenarioRoute::LeaseAfterDurableCommitBeforeAck),
        (PATCH_AFTER_BEFORE_ACK, "patch", ScenarioRoute::PatchAfterDurableCommitBeforeAck),
        (GATE_AFTER_BEFORE_ACK, "gate", ScenarioRoute::GateAfterDurableCommitBeforeAck),
        (
            PROMOTION_AFTER_BEFORE_ACK,
            "promotion",
            ScenarioRoute::PromotionAfterDurableCommitBeforeAck,
        ),
    ];
    for (id, boundary, expected) in after_cases {
        let value = scenario(id, boundary, "after-durable-commit-before-ack", "replayed-committed");
        assert_eq!(ScenarioRoute::from_scenario(&value), Some(expected));
    }
    let corruption = ScenarioDocument {
        id: PROJECTION_CORRUPTION.to_owned(),
        title: "projection corruption".to_owned(),
        fault: FaultDocument::Corruption { target: "projection".to_owned() },
        expected_recovery: "rebuilt-projection".to_owned(),
    };
    assert_eq!(
        ScenarioRoute::from_scenario(&corruption),
        Some(ScenarioRoute::ProjectionCorruption)
    );
    let unsupported = scenario(
        "h1.crash.unknown.before",
        "unknown",
        "before-durable-commit",
        "rolled-back-uncommitted",
    );
    assert_eq!(ScenarioRoute::from_scenario(&unsupported), None);
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
