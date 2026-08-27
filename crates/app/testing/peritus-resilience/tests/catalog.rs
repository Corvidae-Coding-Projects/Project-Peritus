//! Production H1 catalog completeness and stable-coverage integration tests.

use std::collections::BTreeSet;

use peritus_resilience::{
    CommitBoundary, CrashTiming, DaemonLifecyclePhase, FaultInjection,
    H1_PRODUCTION_SCENARIO_COUNT, ScenarioCatalog,
};

#[test]
fn production_catalog_is_complete_unique_and_stably_sorted() {
    let catalog = ScenarioCatalog::h1_production().expect("built-in catalog is valid");
    assert_eq!(catalog.scenarios().len(), H1_PRODUCTION_SCENARIO_COUNT);

    let ids: Vec<_> = catalog.scenarios().iter().map(|scenario| scenario.id().as_str()).collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(ids.iter().copied().collect::<BTreeSet<_>>().len(), ids.len());
}

#[test]
fn production_catalog_has_both_sides_of_every_commit() {
    let catalog = ScenarioCatalog::h1_production().expect("built-in catalog is valid");
    for boundary in CommitBoundary::ALL {
        for timing in [CrashTiming::BeforeDurableCommit, CrashTiming::AfterDurableCommitBeforeAck] {
            assert!(catalog.scenarios().iter().any(|scenario| {
                scenario.fault() == FaultInjection::CommitCrash { boundary, timing }
            }));
        }
    }
}

#[test]
fn production_catalog_kills_daemon_in_every_active_phase() {
    let catalog = ScenarioCatalog::h1_production().expect("built-in catalog is valid");
    for phase in DaemonLifecyclePhase::ALL {
        assert!(
            catalog
                .scenarios()
                .iter()
                .any(|scenario| scenario.fault() == FaultInjection::DaemonKill(phase))
        );
    }
}
