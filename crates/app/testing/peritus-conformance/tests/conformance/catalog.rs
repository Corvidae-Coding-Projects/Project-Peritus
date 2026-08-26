use peritus_conformance::{ConformanceRunner, ConformanceSuite, SuiteStatus, plugin_suite};

use super::harness::{FactoryState, OperationBehavior, TestFactory, TestSubject, block_on};

#[test]
fn unimplemented_catalog_suite_has_a_stable_name() {
    let suites = [plugin_suite::<TestSubject>()];
    let ids = suites.iter().map(|suite| suite.descriptor().id().as_str()).collect::<Vec<_>>();
    assert_eq!(ids, ["peritus.plugin"]);
    assert!(suites.iter().all(|suite| suite.cases().is_empty()));
}

#[test]
fn empty_suite_runs_without_creating_a_subject_or_claiming_conformance() {
    let suite = plugin_suite::<TestSubject>();
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let first = block_on(ConformanceRunner::run(&suite, &factory));
    let second = block_on(ConformanceRunner::run(&suite, &factory));

    assert_eq!(first, second);
    assert_eq!(first.status(), SuiteStatus::Empty);
    assert!(!first.is_conformant());
    assert!(first.cases().is_empty());
    assert_eq!(first.summary().total(), 0);
    assert_eq!(factory.state(), FactoryState::default());
}
