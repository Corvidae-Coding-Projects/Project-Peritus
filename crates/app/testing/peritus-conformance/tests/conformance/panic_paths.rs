use peritus_conformance::{
    BoxedCase, CaseFailure, CaseStatus, ConformanceRunner, ConformanceSuite, FailurePhase,
    PanicFailure, StaticSuite, SuiteDescriptor, SuiteFailure, SuiteId, SuiteStatus,
    TeardownFailure,
};

use super::harness::{
    CaseBehavior, FactoryState, OperationBehavior, TestCase, TestFactory, TestSubject, block_on,
    text,
};

fn suite(cases: Vec<BoxedCase<TestSubject>>) -> StaticSuite<TestSubject> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::new("test.panics").expect("valid suite ID"),
            text("panic boundary tests"),
        ),
        cases,
    )
}

fn assert_panic(failure: &PanicFailure, phase: FailurePhase) {
    assert_eq!(failure.phase(), phase);
    assert!(!failure.messages().is_empty());
}

struct PanickingSuite {
    descriptor: SuiteDescriptor,
    cases: Vec<BoxedCase<TestSubject>>,
    panic_descriptor: bool,
    panic_cases: bool,
}

impl PanickingSuite {
    fn new(panic_descriptor: bool, panic_cases: bool) -> Self {
        Self {
            descriptor: SuiteDescriptor::new(
                SuiteId::new("test.definition-panic").expect("valid suite ID"),
                text("definition panic"),
            ),
            cases: Vec::new(),
            panic_descriptor,
            panic_cases,
        }
    }
}

impl ConformanceSuite<TestSubject> for PanickingSuite {
    fn descriptor(&self) -> &SuiteDescriptor {
        assert!(!self.panic_descriptor, "suite descriptor panic");
        &self.descriptor
    }

    fn cases(&self) -> &[BoxedCase<TestSubject>] {
        assert!(!self.panic_cases, "suite cases panic");
        &self.cases
    }
}

#[test]
fn suite_subject_and_case_metadata_panics_are_invalid_without_partial_cases() {
    let ordinary_factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let suite_descriptor_panic =
        block_on(ConformanceRunner::run(&PanickingSuite::new(true, false), &ordinary_factory));
    assert_eq!(suite_descriptor_panic.status(), SuiteStatus::Invalid);
    assert!(suite_descriptor_panic.suite().is_none());
    assert!(suite_descriptor_panic.cases().is_empty());

    let subject_descriptor_panic =
        block_on(ConformanceRunner::run(&suite(Vec::new()), &TestFactory::descriptor_panics()));
    assert_eq!(subject_descriptor_panic.status(), SuiteStatus::Invalid);
    assert!(subject_descriptor_panic.suite().is_some());
    assert!(subject_descriptor_panic.subject().is_none());

    let cases_panic =
        block_on(ConformanceRunner::run(&PanickingSuite::new(false, true), &ordinary_factory));
    assert_eq!(cases_panic.status(), SuiteStatus::Invalid);
    assert!(cases_panic.cases().is_empty());

    let case_metadata_panic = block_on(ConformanceRunner::run(
        &suite(vec![
            Box::new(TestCase::new("case.first", CaseBehavior::Pass(Vec::new()))),
            Box::new(TestCase::descriptor_panics("case.second")),
        ]),
        &ordinary_factory,
    ));
    assert_eq!(case_metadata_panic.status(), SuiteStatus::Invalid);
    assert!(case_metadata_panic.cases().is_empty());
    assert_eq!(ordinary_factory.state(), FactoryState::default());

    for report in [suite_descriptor_panic, subject_descriptor_panic, cases_panic] {
        let Some(SuiteFailure::Panic(failure)) = report.failure() else {
            panic!("expected suite panic failure");
        };
        assert!(!failure.messages().is_empty());
    }
}

#[test]
fn setup_panics_are_failed_not_executed_and_cannot_claim_pass() {
    for behavior in [
        OperationBehavior::PanicConstruction,
        OperationBehavior::PanicPoll,
        OperationBehavior::PanicNonString,
        OperationBehavior::PanicOnFutureDrop,
    ] {
        let factory = TestFactory::new(behavior, OperationBehavior::Pass);
        let report = block_on(ConformanceRunner::run(
            &suite(vec![Box::new(TestCase::new(
                "case.setup-panic",
                CaseBehavior::Pass(Vec::new()),
            ))]),
            &factory,
        ));
        assert_eq!(report.status(), SuiteStatus::Failed, "behavior {behavior:?}");
        assert!(!report.is_conformant());
        let case = &report.cases()[0];
        assert_eq!(case.status(), CaseStatus::NotExecuted);
        let Some(CaseFailure::Panic(failure)) = case.primary_failure() else {
            panic!("expected setup panic");
        };
        assert_panic(failure, FailurePhase::Setup);
        assert!(case.teardown_failure().is_none());
        assert!(factory.state().torn_down.is_empty());
    }
}

#[test]
fn every_case_panic_form_is_contained_teardown_runs_and_later_cases_continue() {
    for behavior in [
        CaseBehavior::PanicConstruction,
        CaseBehavior::PanicPoll,
        CaseBehavior::PanicNonString,
        CaseBehavior::PanicOnFutureDrop,
    ] {
        let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
        let report = block_on(ConformanceRunner::run(
            &suite(vec![
                Box::new(TestCase::new("case.a-panic", behavior.clone())),
                Box::new(TestCase::new("case.b-pass", CaseBehavior::Pass(Vec::new()))),
            ]),
            &factory,
        ));
        assert_eq!(report.status(), SuiteStatus::Failed);
        let first = &report.cases()[0];
        let Some(CaseFailure::Panic(failure)) = first.primary_failure() else {
            panic!("expected exercise panic");
        };
        assert_panic(failure, FailurePhase::Exercise);
        assert_eq!(report.cases()[1].status(), CaseStatus::Passed);
        assert_eq!(factory.state().torn_down.len(), 2);
    }
}

#[test]
fn teardown_panic_is_preserved_separately_from_a_passing_case() {
    for behavior in [
        OperationBehavior::PanicConstruction,
        OperationBehavior::PanicPoll,
        OperationBehavior::PanicNonString,
        OperationBehavior::PanicOnFutureDrop,
    ] {
        let factory = TestFactory::new(OperationBehavior::Pass, behavior);
        let report = block_on(ConformanceRunner::run(
            &suite(vec![Box::new(TestCase::new(
                "case.teardown-panic",
                CaseBehavior::Pass(Vec::new()),
            ))]),
            &factory,
        ));
        let case = &report.cases()[0];
        assert_eq!(case.status(), CaseStatus::Failed);
        assert!(case.primary_failure().is_none());
        let Some(TeardownFailure::Panic(failure)) = case.teardown_failure() else {
            panic!("expected teardown panic");
        };
        assert_panic(failure, FailurePhase::Teardown);
        assert_eq!(factory.state().torn_down.len(), 1);
    }
}

#[test]
fn panic_payloads_are_normalized_bounded_and_preserve_poll_then_drop_order() {
    let factory = TestFactory::new(OperationBehavior::Pass, OperationBehavior::Pass);
    let report = block_on(ConformanceRunner::run(
        &suite(vec![
            Box::new(TestCase::new("case.long", CaseBehavior::PanicLong)),
            Box::new(TestCase::new("case.payload-drop", CaseBehavior::PanicPayloadDrop)),
            Box::new(TestCase::new("case.two-panics", CaseBehavior::PanicPollAndDrop)),
        ]),
        &factory,
    ));
    let Some(CaseFailure::Panic(long)) = report.cases()[0].primary_failure() else {
        panic!("expected long panic");
    };
    assert!(long.messages()[0].was_oversized());
    assert_eq!(long.messages()[0].original_length(), 8_192);
    assert_eq!(
        long.messages()[0].as_str(),
        "panic message omitted because it exceeded report limits"
    );
    assert_eq!(long.messages()[0].text().as_str(), long.messages()[0].as_str());

    let Some(CaseFailure::Panic(payload_drop)) = report.cases()[1].primary_failure() else {
        panic!("expected payload-drop panic");
    };
    assert_eq!(payload_drop.messages().len(), 2);
    assert_eq!(payload_drop.messages()[0].as_str(), "non-string panic payload");
    assert_eq!(payload_drop.messages()[1].as_str(), "panic payload drop");

    let Some(CaseFailure::Panic(two)) = report.cases()[2].primary_failure() else {
        panic!("expected combined panic");
    };
    assert_eq!(two.messages().len(), 2);
    assert_eq!(two.messages()[0].as_str(), "future poll panic");
    assert_eq!(two.messages()[1].as_str(), "future drop after poll panic");
}
