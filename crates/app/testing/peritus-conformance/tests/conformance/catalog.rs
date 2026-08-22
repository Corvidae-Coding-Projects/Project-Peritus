use peritus_conformance::{
    ConformanceRunner, ConformanceSuite, SuiteStatus, journal_suite, plugin_suite, protocol_suite,
    provider_suite, replay_suite, sandbox_suite, tool_suite,
};

use super::harness::{FactoryState, OperationBehavior, TestFactory, TestSubject, block_on};

#[test]
fn seven_catalog_suites_have_stable_unique_names() {
    let suites = [
        provider_suite::<TestSubject>(),
        tool_suite(),
        plugin_suite(),
        sandbox_suite(),
        journal_suite(),
        protocol_suite(),
        replay_suite(),
    ];
    let ids = suites.iter().map(|suite| suite.descriptor().id().as_str()).collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "peritus.provider",
            "peritus.tool",
            "peritus.plugin",
            "peritus.sandbox",
            "peritus.journal",
            "peritus.protocol",
            "peritus.replay",
        ]
    );
    assert!(suites.iter().all(|suite| suite.cases().is_empty()));
}

#[test]
fn empty_suite_runs_without_creating_a_subject_or_claiming_conformance() {
    let suite = provider_suite::<TestSubject>();
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
