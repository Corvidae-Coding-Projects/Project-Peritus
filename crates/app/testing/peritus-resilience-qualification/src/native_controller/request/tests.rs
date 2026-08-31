use super::route::{
    BLOB_AFTER_BEFORE_ACK, BLOB_BEFORE, BLOB_CORRUPTION, BLOB_FINALIZE_DISK_EXHAUSTION,
    DaemonPhase, GATE_AFTER_BEFORE_ACK, GATE_BEFORE, JOURNAL_AFTER_BEFORE_ACK,
    JOURNAL_APPEND_DISK_EXHAUSTION, JOURNAL_BEFORE, JOURNAL_CORRUPTION, LEASE_AFTER_BEFORE_ACK,
    LEASE_BEFORE, PATCH_AFTER_BEFORE_ACK, PATCH_BEFORE, PROJECTION_CORRUPTION,
    PROMOTION_AFTER_BEFORE_ACK, PROMOTION_BEFORE, PROVIDER_DEATH, PROVIDER_RETRY_EXHAUSTION,
    SNAPSHOT_AFTER_BEFORE_ACK, SNAPSHOT_BEFORE, SNAPSHOT_CORRUPTION, ScenarioRoute, TOOL_DEATH,
    TOOL_RETRY_EXHAUSTION, WORKER_DEATH, WORKER_RETRY_EXHAUSTION,
};
use super::{FaultDocument, ScenarioDocument};

#[test]
fn the_process_and_dependency_routes_are_admitted() {
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
    let journal_corruption = ScenarioDocument {
        id: JOURNAL_CORRUPTION.to_owned(),
        title: "journal corruption".to_owned(),
        fault: FaultDocument::Corruption { target: "journal".to_owned() },
        expected_recovery: "failed-closed".to_owned(),
    };
    assert_eq!(
        ScenarioRoute::from_scenario(&journal_corruption),
        Some(ScenarioRoute::JournalCorruption)
    );
    let blob_corruption = ScenarioDocument {
        id: BLOB_CORRUPTION.to_owned(),
        title: "blob corruption".to_owned(),
        fault: FaultDocument::Corruption { target: "blob".to_owned() },
        expected_recovery: "quarantined-corruption".to_owned(),
    };
    assert_eq!(ScenarioRoute::from_scenario(&blob_corruption), Some(ScenarioRoute::BlobCorruption));
    let snapshot_corruption = ScenarioDocument {
        id: SNAPSHOT_CORRUPTION.to_owned(),
        title: "snapshot corruption".to_owned(),
        fault: FaultDocument::Corruption { target: "snapshot".to_owned() },
        expected_recovery: "quarantined-corruption".to_owned(),
    };
    assert_eq!(
        ScenarioRoute::from_scenario(&snapshot_corruption),
        Some(ScenarioRoute::SnapshotCorruption)
    );
    for (id, dependency, expected) in [
        (PROVIDER_DEATH, "provider", ScenarioRoute::ProviderDeath),
        (TOOL_DEATH, "tool", ScenarioRoute::ToolDeath),
        (WORKER_DEATH, "worker", ScenarioRoute::WorkerDeath),
    ] {
        let value = ScenarioDocument {
            id: id.to_owned(),
            title: "dependency death".to_owned(),
            fault: FaultDocument::DependencyDeath { dependency: dependency.to_owned() },
            expected_recovery: "reconciled-owned-work".to_owned(),
        };
        assert_eq!(ScenarioRoute::from_scenario(&value), Some(expected));
    }
    for (id, dependency, expected) in [
        (PROVIDER_RETRY_EXHAUSTION, "provider", ScenarioRoute::ProviderRetryExhaustion),
        (TOOL_RETRY_EXHAUSTION, "tool", ScenarioRoute::ToolRetryExhaustion),
        (WORKER_RETRY_EXHAUSTION, "worker", ScenarioRoute::WorkerRetryExhaustion),
    ] {
        let value = ScenarioDocument {
            id: id.to_owned(),
            title: "dependency retry exhaustion".to_owned(),
            fault: FaultDocument::RetryExhaustion { dependency: dependency.to_owned() },
            expected_recovery: "retry-budget-exhausted".to_owned(),
        };
        assert_eq!(ScenarioRoute::from_scenario(&value), Some(expected));
    }
    let unsupported = scenario(
        "h1.crash.unknown.before",
        "unknown",
        "before-durable-commit",
        "rolled-back-uncommitted",
    );
    assert_eq!(ScenarioRoute::from_scenario(&unsupported), None);
}

#[test]
fn all_eleven_daemon_lifecycle_routes_require_exact_catalog_identity() {
    for (phase, expected) in [
        ("writer-pending", DaemonPhase::WriterPending),
        ("writer-active", DaemonPhase::WriterActive),
        ("gates-pending", DaemonPhase::GatesPending),
        ("gates-active", DaemonPhase::GatesActive),
        ("review-pending", DaemonPhase::ReviewPending),
        ("review-active", DaemonPhase::ReviewActive),
        ("fixer-pending", DaemonPhase::FixerPending),
        ("fixer-active", DaemonPhase::FixerActive),
        ("revision-advancing", DaemonPhase::RevisionAdvancing),
        ("evaluating-acceptance", DaemonPhase::EvaluatingAcceptance),
        ("kernel-acceptance-pending", DaemonPhase::KernelAcceptancePending),
    ] {
        let scenario = ScenarioDocument {
            id: format!("h1.daemon-kill.{phase}"),
            title: "daemon lifecycle kill".to_owned(),
            fault: FaultDocument::DaemonKill { phase: phase.to_owned() },
            expected_recovery: "reconciled-owned-work".to_owned(),
        };
        assert_eq!(
            ScenarioRoute::from_scenario(&scenario),
            Some(ScenarioRoute::DaemonLifecycle(expected))
        );
        let wrong_id = ScenarioDocument { id: "h1.daemon-kill.wrong".to_owned(), ..scenario };
        assert_eq!(ScenarioRoute::from_scenario(&wrong_id), None);
    }
}

#[test]
fn the_finalize_time_artifact_quota_route_is_admitted() {
    let scenario = ScenarioDocument {
        id: BLOB_FINALIZE_DISK_EXHAUSTION.to_owned(),
        title: "blob finalize quota exhaustion".to_owned(),
        fault: FaultDocument::DiskExhaustion { scope: "blob-finalize".to_owned() },
        expected_recovery: "discarded-unreferenced".to_owned(),
    };
    assert_eq!(
        ScenarioRoute::from_scenario(&scenario),
        Some(ScenarioRoute::BlobFinalizeDiskExhaustion)
    );
}

#[test]
fn the_authoritative_journal_page_exhaustion_route_is_admitted() {
    let scenario = ScenarioDocument {
        id: JOURNAL_APPEND_DISK_EXHAUSTION.to_owned(),
        title: "journal append storage exhaustion".to_owned(),
        fault: FaultDocument::DiskExhaustion { scope: "journal-append".to_owned() },
        expected_recovery: "rolled-back-uncommitted".to_owned(),
    };
    assert_eq!(
        ScenarioRoute::from_scenario(&scenario),
        Some(ScenarioRoute::JournalAppendDiskExhaustion)
    );
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
