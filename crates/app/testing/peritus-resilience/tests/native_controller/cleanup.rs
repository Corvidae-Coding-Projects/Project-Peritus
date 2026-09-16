use super::*;

#[test]
fn response_queued_after_controller_exit_does_not_wait_twice() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&controller_with_cleanup(
        ABC_SHA256,
        true,
        r#"
      controller=$$
      printf '{"schema_version":1,"stage":"%s","sequence":%s,"instance_id":"%s","scenario_id":"%s","request_sha256":"%s","payload":{"resources_released":true,"owned_work_remaining":0,"cleanup_steps":1}}' "$stage" "$sequence" "$instance" "$scenario" "$request_sha"
      (while kill -0 "$controller" 2>/dev/null; do sleep 0.01; done; printf '\n') &
      exit 0
"#,
    ));
    let factory = fixture.factory();
    let report = run_one_scenario(&factory);

    assert!(
        report.cases()[0].failures().is_empty(),
        "cleanup race failure: {:?}",
        report.cases()[0].failures()
    );
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 1);
}

#[test]
fn oversized_queued_response_preserves_the_output_limit_error() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&controller_with_cleanup(
        ABC_SHA256,
        true,
        r#"
      controller=$$
      dd if=/dev/zero bs=16384 count=1 2>/dev/null | tr '\000' ' '
      printf '{"schema_version":1,"stage":"%s","sequence":%s,"instance_id":"%s","scenario_id":"%s","request_sha256":"%s","payload":{"resources_released":true,"owned_work_remaining":0,"cleanup_steps":1}}' "$stage" "$sequence" "$instance" "$scenario" "$request_sha"
      (while kill -0 "$controller" 2>/dev/null; do sleep 0.01; done; printf '\n') &
      exit 0
"#,
    ));
    let limits = NativeControllerLimits::new(Duration::from_secs(5), 32 * 1024, 8 * 1024, 64)
        .expect("focused native limits");
    let factory = fixture.factory_with_limits(limits);
    let report = run_one_scenario(&factory);
    let failures = report.cases()[0].failures();

    assert!(
        failures.iter().any(|failure| matches!(
            failure,
            ScenarioFailure::Subject { error, .. }
                if error.context().as_str().contains("output byte limit")
        )),
        "queued oversized response lost its limit diagnostic: {failures:?}"
    );
    assert!(
        failures.iter().all(|failure| !matches!(
            failure,
            ScenarioFailure::Subject { error, .. }
                if error.context().as_str().contains("native controller was released")
        )),
        "queued oversized response attempted repeated cleanup: {failures:?}"
    );
}

fn run_one_scenario(factory: &NativeResilienceFactory) -> peritus_resilience::QualificationReport {
    let production = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let catalog = ScenarioCatalog::custom(vec![production.scenarios()[0].clone()])
        .expect("focused H1 catalog");
    block_on(QualificationRunner::run(factory.config(), &catalog, factory))
}
