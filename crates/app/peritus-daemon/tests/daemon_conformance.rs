//! Production `peritusd` subprocess coverage for the runtime-neutral G0 contract.

#![cfg(unix)]

mod daemon_conformance_support;

use peritus_conformance::{
    ConformanceRunner, DAEMON_SCENARIOS, SuiteStatus, daemon_scenario_suite,
};

use daemon_conformance_support::{
    BinaryDaemonFactory, BinaryDaemonSubject, blob_commit_crash_recovery, blocker_for,
    journal_before_crash_recovery, reachable_scenarios,
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
