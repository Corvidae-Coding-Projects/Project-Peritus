use super::*;

#[test]
fn cleanup_waits_for_a_short_lived_owned_descendant() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&controller_with_cleanup(
        ABC_SHA256,
        true,
        "sleep 0.25 </dev/null >/dev/null 2>&1 &",
    ));
    let factory = fixture.factory();
    let production = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let catalog = ScenarioCatalog::custom(vec![production.scenarios()[0].clone()])
        .expect("focused H1 catalog");
    let report = block_on(QualificationRunner::run(factory.config(), &catalog, &factory));

    assert!(
        report.cases()[0].failures().is_empty(),
        "short-lived descendant did not drain: {:?}",
        report.cases()[0].failures(),
    );
    assert!(
        report.cases()[0]
            .cleanup()
            .is_some_and(peritus_resilience::CleanupObservation::resources_released),
    );
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
}
