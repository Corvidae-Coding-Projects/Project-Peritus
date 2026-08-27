//! Production `peritusd` subprocess coverage for the runtime-neutral G0 contract.

#![cfg(unix)]

mod daemon_conformance_support;

use peritus_conformance::{
    ConformanceRunner, DAEMON_SCENARIOS, DaemonScenario, SuiteStatus, daemon_scenario_suite,
};

use daemon_conformance_support::{
    BinaryDaemonFactory, BinaryDaemonSubject, blocker_for, reachable_scenarios,
};

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
    assert_eq!(reachable.len(), 26);
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
fn currently_unreachable_cases_have_exact_typed_blockers() {
    let blocked = DAEMON_SCENARIOS
        .iter()
        .copied()
        .filter_map(|scenario| blocker_for(scenario).map(|detail| (scenario, detail)))
        .collect::<Vec<_>>();
    assert_eq!(blocked.len(), 2);
    assert!(blocked.iter().all(|(_, detail)| detail.contains("peritusd")));
    assert!(blocked.iter().any(|(scenario, _)| *scenario == DaemonScenario::OutboxCrash));
}
