use super::*;
use peritus_resilience::QualificationReport;

pub fn assert_failed_subjects_are_clean(report: &QualificationReport, expected: &str) {
    for case in report.cases() {
        let scenario = case.scenario().id().as_str();
        assert!(
            case.failures().iter().any(|failure| {
                matches!(
                    failure,
                    ScenarioFailure::Subject { error, .. }
                        if error.context().as_str().contains(expected)
                )
            }),
            "{scenario}: expected {expected:?} rejection; failures={:?}; cleanup={:?}",
            case.failures(),
            case.cleanup(),
        );
        assert!(
            case.cleanup().is_some_and(peritus_resilience::CleanupObservation::resources_released),
            "{scenario}: cleanup after {expected:?} rejection failed; failures={:?}; cleanup={:?}",
            case.failures(),
            case.cleanup(),
        );
    }
}

#[test]
fn rejection_failure_identifies_the_exact_case_and_observed_state() {
    assert_failure_diagnostic(true, "");
}

#[test]
fn cleanup_failure_identifies_the_exact_case_and_observed_state() {
    assert_failure_diagnostic(false, "printf 'deliberate fixture cleanup failure\\n' >&2; exit 73");
}

fn assert_failure_diagnostic(bind_prepare: bool, cleanup_command: &str) {
    let _native_test = native_test_guard();
    let production = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let catalog = ScenarioCatalog::custom(vec![production.scenarios()[0].clone()])
        .expect("focused diagnostic fixture catalog");
    let fixture =
        NativeFixture::new(&controller_with_cleanup(ABC_SHA256, bind_prepare, cleanup_command));
    let factory = fixture.factory();
    let report = block_on(QualificationRunner::run(factory.config(), &catalog, &factory));
    let payload = std::panic::catch_unwind(|| {
        assert_failed_subjects_are_clean(&report, "stale");
    })
    .expect_err("wrong rejection or incomplete cleanup must still fail");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("text case diagnostic");
    for detail in [catalog.scenarios()[0].id().as_str(), "stale", "failures=", "cleanup="] {
        assert!(message.contains(detail), "missing {detail:?} in diagnostic: {message}");
    }
    if !bind_prepare {
        assert!(message.contains("deliberate fixture cleanup failure"), "{message}");
    }
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
}
