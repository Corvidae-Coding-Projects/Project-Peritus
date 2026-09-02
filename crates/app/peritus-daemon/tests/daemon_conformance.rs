//! Production `peritusd` subprocess coverage for the runtime-neutral G0 contract.

#![cfg(unix)]

mod daemon_conformance_support;

use peritus_conformance::{
    ConformanceRunner, DAEMON_SCENARIOS, SuiteStatus, daemon_scenario_suite,
};

use daemon_conformance_support::{
    BinaryDaemonFactory, BinaryDaemonSubject, blob_commit_crash_recovery, blob_corruption_recovery,
    blocker_for, dependency_failure_recovery, evidence_corruption_recovery,
    gate_commit_crash_recovery, journal_append_exhaustion_recovery, journal_before_crash_recovery,
    journal_corruption_recovery, lease_commit_crash_recovery, patch_commit_crash_recovery,
    projection_corruption_recovery, promotion_commit_crash_recovery,
    promotion_evidence_corruption_recovery, reachable_scenarios, snapshot_commit_crash_recovery,
    snapshot_corruption_recovery, snapshot_quota_exhaustion_recovery,
};

#[test]
fn journal_append_plan_dies_cleanly_before_durable_commit() {
    journal_before_crash_recovery().expect("real pre-commit journal crash recovery");
}

#[test]
fn artifact_commit_recovers_on_both_sides_of_publication() {
    blob_commit_crash_recovery().expect("real artifact commit crash recovery");
}

#[test]
fn corrupt_referenced_artifact_is_quarantined_and_made_unavailable() {
    blob_corruption_recovery().expect("real artifact corruption containment");
}

#[test]
fn snapshot_commit_recovers_on_both_sides_of_retained_ref_publication() {
    snapshot_commit_crash_recovery().expect("real Git snapshot commit crash recovery");
}

#[test]
fn corrupt_snapshot_reference_is_quarantined_before_reuse() {
    snapshot_corruption_recovery().expect("real Git snapshot corruption containment");
}

#[test]
fn snapshot_manifest_quota_releases_the_unpublished_reference() {
    snapshot_quota_exhaustion_recovery().expect("real snapshot manifest quota recovery");
}

#[test]
fn lease_commit_recovers_on_both_sides_of_durable_projection_install() {
    lease_commit_crash_recovery().expect("real lease commit crash recovery");
}

#[test]
fn patch_commit_recovers_on_both_sides_of_durable_application() {
    patch_commit_crash_recovery().expect("real patch commit crash recovery");
}

#[test]
fn gate_commit_recovers_on_both_sides_of_atomic_checkpoint_install() {
    gate_commit_crash_recovery().expect("real gate commit crash recovery");
}

#[test]
fn promotion_commit_recovers_on_both_sides_of_atomic_activation() {
    promotion_commit_crash_recovery().expect("real promotion commit crash recovery");
}

#[test]
fn corrupt_projection_is_replaced_during_fresh_startup() {
    projection_corruption_recovery().expect("real projection corruption recovery");
}

#[test]
fn corrupt_journal_stops_fresh_startup_before_authority_mutation() {
    journal_corruption_recovery().expect("real journal corruption recovery");
}

#[test]
fn corrupt_acceptance_evidence_is_quarantined_before_reuse() {
    evidence_corruption_recovery().expect("real acceptance evidence containment");
}

#[test]
fn corrupt_harness_activation_evidence_is_quarantined_without_pointer_mutation() {
    promotion_evidence_corruption_recovery().expect("real harness activation evidence containment");
}

#[test]
fn journal_page_exhaustion_rolls_back_and_allows_fresh_inspection() {
    journal_append_exhaustion_recovery().expect("real journal page-exhaustion recovery");
}

#[test]
fn dependency_death_and_retry_exhaustion_reconcile_real_owned_work() {
    dependency_failure_recovery().expect("real dependency failure recovery");
}

#[test]
fn every_publicly_reachable_daemon_case_passes_against_peritusd() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build current-thread conformance executor");
    for scenario in reachable_scenarios() {
        let report = runtime.block_on(ConformanceRunner::run(
            &daemon_scenario_suite::<BinaryDaemonSubject>(*scenario),
            &BinaryDaemonFactory::new(),
        ));
        assert_eq!(
            report.status(),
            SuiteStatus::Passed,
            "production subprocess failed {scenario:?}: {report:?}",
        );
        assert_eq!(report.summary().total(), 1);
    }
}

#[test]
fn public_coverage_inventory_partitions_the_complete_contract() {
    let reachable = reachable_scenarios();
    assert_eq!(DAEMON_SCENARIOS.len(), 28);
    assert_eq!(reachable.len(), 28);
    for scenario in DAEMON_SCENARIOS {
        let is_reachable = reachable.contains(scenario);
        let blocker = blocker_for(*scenario);
        assert_ne!(is_reachable, blocker.is_some(), "coverage overlap or hole for {scenario:?}");
        if let Some(detail) = blocker {
            assert!(!detail.is_empty(), "empty blocker for {scenario:?}");
        }
    }
}

#[test]
fn no_contract_case_remains_blocked_from_the_public_binary() {
    let blocked = DAEMON_SCENARIOS
        .iter()
        .copied()
        .filter_map(|scenario| blocker_for(scenario).map(|detail| (scenario, detail)))
        .collect::<Vec<_>>();
    assert!(blocked.is_empty(), "complete G0 binary coverage left blockers: {blocked:?}");
}
