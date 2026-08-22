use peritus_conformance::{
    BoxedCase, CaseFailure, CaseStatus, ConformanceRunner, FailureKind, Observation, ObservationId,
    ObservationValue, StaticSuite, SuiteDescriptor, SuiteFailure, SuiteId, SuiteStatus,
    TeardownFailure,
};

use super::harness::{
    CaseBehavior, FactoryState, OperationBehavior, TestCase, TestFactory, TestSubject, block_on,
    text,
};

fn suite(cases: Vec<BoxedCase<TestSubject>>) -> StaticSuite<TestSubject> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::new("test.runner").expect("valid suite ID"),
            text("runner contract tests"),
        ),
        cases,
    )
}

#[test]
fn cases_run_in_id_order_with_fresh_subjects_and_exact_teardown() {
    let suite = suite(vec![
        Box::new(TestCase::new("case.zulu", CaseBehavior::Pass(Vec::new()))),
        Box::new(TestCase::new("case.alpha", CaseBehavior::Pass(Vec::new()))),
        Box::new(TestCase::new("case.middle", CaseBehavior::Pass(Vec::new()))),
    ]);
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let report = block_on(ConformanceRunner::run(&suite, &factory));

    assert_eq!(report.status(), SuiteStatus::Passed);
    assert!(report.is_conformant());
    assert_eq!(report.summary().passed(), 3);
    assert_eq!(report.summary().failed(), 0);
    assert_eq!(report.summary().not_executed(), 0);
    assert_eq!(report.summary().contract_violation_cases(), 0);
    assert_eq!(report.summary().infrastructure_failure_cases(), 0);
    let ids = report.cases().iter().map(|case| case.descriptor().id().as_str()).collect::<Vec<_>>();
    assert_eq!(ids, ["case.alpha", "case.middle", "case.zulu"]);
    let state = factory.state();
    assert_eq!(state.created_for, ids);
    assert_eq!(
        state.torn_down,
        [("case.alpha".to_owned(), 1), ("case.middle".to_owned(), 2), ("case.zulu".to_owned(), 3),]
    );
}

#[test]
fn duplicate_ids_invalidate_before_any_subject_is_created() {
    let suite = suite(vec![
        Box::new(TestCase::new("case.duplicate", CaseBehavior::Pass(Vec::new()))),
        Box::new(TestCase::new("case.duplicate", CaseBehavior::Fail)),
    ]);
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let report = block_on(ConformanceRunner::run(&suite, &factory));

    assert_eq!(report.status(), SuiteStatus::Invalid);
    assert!(report.cases().is_empty());
    assert_eq!(factory.state(), FactoryState::default());
    let SuiteFailure::DuplicateCaseId(failure) = report.failure().expect("suite failure") else {
        panic!("expected duplicate case ID failure");
    };
    assert_eq!(failure.id().as_str(), "case.duplicate");
}

#[test]
fn typed_setup_failure_is_not_executed_and_has_no_teardown() {
    let suite = suite(vec![Box::new(TestCase::new("case.setup", CaseBehavior::Pass(Vec::new())))]);
    let factory = TestFactory::new(OperationBehavior::TypedFailure, OperationBehavior::Pass);
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    let case = &report.cases()[0];

    assert_eq!(report.status(), SuiteStatus::Failed);
    assert_eq!(case.status(), CaseStatus::NotExecuted);
    assert!(matches!(case.primary_failure(), Some(CaseFailure::Setup(_))));
    assert!(case.teardown_failure().is_none());
    assert!(factory.state().torn_down.is_empty());
    assert_eq!(report.summary().not_executed(), 1);
    assert!(!case.has_failure_kind(FailureKind::ContractViolation));
    assert!(case.has_failure_kind(FailureKind::Infrastructure));
    assert_eq!(report.summary().contract_violation_cases(), 0);
    assert_eq!(report.summary().infrastructure_failure_cases(), 1);
}

#[test]
fn assertion_and_teardown_failures_are_both_preserved() {
    let suite = suite(vec![Box::new(TestCase::new("case.fail", CaseBehavior::Fail))]);
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::TypedFailure);
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    let case = &report.cases()[0];

    assert_eq!(case.status(), CaseStatus::Failed);
    assert!(matches!(
        case.primary_failure(),
        Some(CaseFailure::Assertion(failure)) if failure.code().as_str() == "TEST-ASSERTION"
    ));
    assert!(matches!(
        case.teardown_failure(),
        Some(TeardownFailure::Subject(failure)) if failure.code().as_str() == "TEST-TEARDOWN"
    ));
    assert!(case.has_failure_kind(FailureKind::ContractViolation));
    assert!(case.has_failure_kind(FailureKind::Infrastructure));
    assert_eq!(report.summary().contract_violation_cases(), 1);
    assert_eq!(report.summary().infrastructure_failure_cases(), 1);
    assert_eq!(factory.state().torn_down.len(), 1);
}

#[test]
fn multiple_infrastructure_failures_in_one_case_count_once() {
    let suite =
        suite(vec![Box::new(TestCase::new("case.infrastructure", CaseBehavior::PanicPoll))]);
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::TypedFailure);
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    let case = &report.cases()[0];

    assert!(matches!(case.primary_failure(), Some(CaseFailure::Panic(_))));
    assert!(matches!(case.teardown_failure(), Some(TeardownFailure::Subject(_))));
    assert_eq!(report.summary().contract_violation_cases(), 0);
    assert_eq!(report.summary().infrastructure_failure_cases(), 1);
}

#[test]
fn observations_keep_explicit_order_and_pending_future_completes() {
    let observations = vec![
        Observation::new(
            ObservationId::new("stream.second").expect("valid observation ID"),
            ObservationValue::Unsigned(2),
        ),
        Observation::new(
            ObservationId::new("stream.first").expect("valid observation ID"),
            ObservationValue::Unsigned(1),
        ),
    ];
    let suite = suite(vec![
        Box::new(TestCase::new("case.observations", CaseBehavior::Pass(observations.clone()))),
        Box::new(TestCase::new("case.pending", CaseBehavior::PendingOnce)),
    ]);
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let report = block_on(ConformanceRunner::run(&suite, &factory));

    assert_eq!(report.status(), SuiteStatus::Passed);
    assert_eq!(report.cases()[0].observations(), observations);
    assert_eq!(report.cases()[1].status(), CaseStatus::Passed);
}

#[test]
fn repeated_deterministic_runs_produce_equal_reports() {
    let suite = suite(vec![Box::new(TestCase::new("case.stable", CaseBehavior::Pass(Vec::new())))]);
    let first_factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let second_factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);

    let first = block_on(ConformanceRunner::run(&suite, &first_factory));
    let second = block_on(ConformanceRunner::run(&suite, &second_factory));
    assert_eq!(first, second);
}
