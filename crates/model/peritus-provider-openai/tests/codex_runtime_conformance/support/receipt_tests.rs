//! Multi-owner failures retain earlier observations without retrying or inventing counts.

use peritus_conformance::{ObservationValue, ProviderScenario};

use super::super::observations;
use super::tests::{text, value};
use super::{
    FakeExecutable, ForeignProbe, Probe, RecoveryProbe, fixtures, profile, recovery, request,
};

#[test]
fn unstarted_foreign_receipt_proves_zero_requests_and_explicit_cleanup() {
    let scenario = ProviderScenario::AdapterIsolation;
    let helper = FakeExecutable::install(scenario).unwrap();
    let directory = helper.directory.path().to_owned();
    let foreign = ForeignProbe::from_helper(helper, profile(scenario, 0xe6).unwrap()).unwrap();
    let receipt = foreign.finish().expect("unstarted private owner finishes");
    let evidence = receipt.evidence();
    assert_eq!(value(&evidence, "foreign.requests"), &ObservationValue::Unsigned(0));
    assert_eq!(value(&evidence, "foreign.trace-present"), &ObservationValue::Boolean(false));
    assert_eq!(value(&evidence, "foreign.directory-removed"), &ObservationValue::Boolean(true));
    assert!(!directory.exists());
}

#[test]
fn foreign_configuration_failure_retains_original_cause_and_cleanup() {
    let scenario = ProviderScenario::AdapterIsolation;
    let helper = FakeExecutable::install(scenario).unwrap();
    let directory = helper.directory.path().to_owned();
    std::fs::remove_file(helper.path()).unwrap();
    let error = ForeignProbe::from_helper(helper, profile(scenario, 0xe7).unwrap())
        .err()
        .expect("missing executable cannot configure a provider");
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "executable.pin");
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
    assert!(!directory.exists());
    assert!(!format!("{evidence:?}").contains(&directory.to_string_lossy().into_owned()));
}

#[test]
fn isolation_validation_retains_selected_and_unstarted_foreign_receipts() {
    let fixture = fixtures::fixture(ProviderScenario::AdapterIsolation);
    let mut probe = Probe::run(&fixture).unwrap();
    probe.trace.push("auth".to_owned());
    let error = observations::isolation(&probe, &fixture)
        .expect_err("two observed auth requests cannot prove single-adapter effects");
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "verify.isolation");
    assert_eq!(text(&evidence, "selected.probe.terminal"), "completed");
    assert_eq!(value(&evidence, "selected.probe.auth-requests"), &ObservationValue::Unsigned(2));
    assert_eq!(
        value(&evidence, "selected.probe.directory-removed"),
        &ObservationValue::Boolean(true)
    );
    assert_eq!(value(&evidence, "foreign.requests"), &ObservationValue::Unsigned(0));
    assert_eq!(value(&evidence, "foreign.directory-removed"), &ObservationValue::Boolean(true));
}

#[test]
fn capability_second_fault_retains_first_receipt_and_does_not_run_third() {
    let fixture = fixtures::fixture(ProviderScenario::CapabilityHonesty);
    let mut calls = 0;
    let error = observations::capabilities::observe_with(&fixture, |fixture| {
        calls += 1;
        if calls == 1 {
            return Probe::run(fixture);
        }
        assert_eq!(calls, 2, "a failed probe cannot trigger a hidden retry");
        let scenario = fixture.scenario();
        let profile = profile(scenario, 0xe8).unwrap();
        let request = request(&profile, true, None).unwrap();
        let helper = FakeExecutable::install(scenario).unwrap();
        std::fs::create_dir(helper.trace_path()).unwrap();
        Probe::run_installed(scenario, profile, request, helper)
    })
    .expect_err("second trace fault must remain a failed exercise");
    assert_eq!(calls, 2);
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "trace.read");
    assert_eq!(text(&evidence, "probe.scope"), "capability.second");
    assert_eq!(text(&evidence, "capability.first.probe.terminal"), "completed");
    assert_eq!(
        value(&evidence, "capability.first.probe.auth-requests"),
        &ObservationValue::Unsigned(1)
    );
    assert_eq!(
        value(&evidence, "capability.first.probe.directory-removed"),
        &ObservationValue::Boolean(true)
    );
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
    assert!(!evidence.iter().any(|fact| fact.id().as_str() == "probe.turn-requests"));
}

#[test]
fn capability_validation_retains_all_three_completed_probe_receipts() {
    let fixture = fixtures::fixture(ProviderScenario::CapabilityHonesty);
    let mut calls = 0;
    let error = observations::capabilities::observe_with(&fixture, |fixture| {
        calls += 1;
        let mut probe = Probe::run(fixture)?;
        if calls == 2 {
            probe.trace.push("turn".to_owned());
        }
        Ok(probe)
    })
    .expect_err("an extra observed request violates capability effects");
    assert_eq!(calls, 3);
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "verify.capability-effects");
    for scope in ["capability.first", "capability.second", "capability.third"] {
        assert_eq!(text(&evidence, &format!("{scope}.probe.terminal")), "completed");
        assert_eq!(
            value(&evidence, &format!("{scope}.probe.directory-removed")),
            &ObservationValue::Boolean(true)
        );
    }
    assert_eq!(
        value(&evidence, "capability.second.probe.turn-requests"),
        &ObservationValue::Unsigned(2)
    );
}

#[test]
fn recovery_second_start_failure_keeps_first_terminal_and_final_cleanup() {
    let scenario = ProviderScenario::RateLimitRetryAfter;
    let fixture = fixtures::fixture(scenario);
    let profile = profile(scenario, 0xe9).unwrap();
    let request = request(&profile, false, None).unwrap();
    let bytes = u64::try_from(request.canonical_bytes().unwrap().len()).unwrap();
    let helper = FakeExecutable::install(scenario).unwrap();
    let directory = helper.directory.path().to_owned();
    let trace_path = helper.trace_path();
    let mut calls = 0;
    let result = recovery::attempts(
        &fixture,
        request,
        bytes,
        &helper,
        profile,
        &trace_path,
        |provider, request, scenario, trace| {
            calls += 1;
            let result = super::run_provider(provider, request, scenario, trace);
            if calls == 1 {
                std::fs::remove_file(helper.path()).unwrap();
            }
            result
        },
    );
    let error =
        helper.finish(result, Vec::new()).expect_err("removed alias cannot start attempt two");
    assert_eq!(calls, 2);
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "provider.start");
    assert_eq!(text(&evidence, "probe.scope"), "recovery.second");
    assert_eq!(text(&evidence, "recovery.first.probe.terminal"), "failed");
    assert_eq!(text(&evidence, "recovery.first.probe.failure-category"), "Provider");
    assert_eq!(text(&evidence, "probe.terminal"), "absent");
    assert_eq!(value(&evidence, "probe.turn-requests"), &ObservationValue::Unsigned(1));
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
    assert!(!directory.exists());
}

#[test]
fn recovery_final_trace_failure_keeps_both_real_attempt_terminals() {
    let scenario = ProviderScenario::RateLimitRetryAfter;
    let fixture = fixtures::fixture(scenario);
    let profile = profile(scenario, 0xea).unwrap();
    let request = request(&profile, false, None).unwrap();
    let bytes = u64::try_from(request.canonical_bytes().unwrap().len()).unwrap();
    let helper = FakeExecutable::install(scenario).unwrap();
    let directory = helper.directory.path().to_owned();
    let trace_path = helper.trace_path();
    let (first, second, plan) = recovery::attempts(
        &fixture,
        request,
        bytes,
        &helper,
        profile,
        &trace_path,
        super::run_provider,
    )
    .unwrap();
    let receipts = recovery::attempt_evidence(&first, &second);
    std::fs::remove_file(&trace_path).unwrap();
    std::fs::create_dir(&trace_path).unwrap();
    let error = helper
        .finish(Ok((first, second, plan)), receipts)
        .expect_err("unreadable final trace cannot qualify recovery");
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "trace.read");
    assert_eq!(text(&evidence, "recovery.first.probe.terminal"), "failed");
    assert_eq!(text(&evidence, "recovery.second.probe.terminal"), "completed");
    assert_eq!(value(&evidence, "probe.trace-readable"), &ObservationValue::Boolean(false));
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
    assert!(!evidence.iter().any(|fact| fact.id().as_str() == "probe.turn-requests"));
    assert!(!directory.exists());
}

#[test]
fn recovery_validation_keeps_both_terminals_and_original_failed_predicate() {
    let fixture = fixtures::fixture(ProviderScenario::RateLimitRetryAfter);
    let mut probe = RecoveryProbe::run(&fixture).unwrap();
    probe.trace.push("turn".to_owned());
    let error = observations::observe_recovery(&probe, &fixture)
        .expect_err("three observed turns cannot qualify two-attempt recovery");
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "verify.recovery");
    assert_eq!(text(&evidence, "recovery.first.probe.terminal"), "failed");
    assert_eq!(text(&evidence, "recovery.second.probe.terminal"), "completed");
    assert_eq!(value(&evidence, "probe.turn-requests"), &ObservationValue::Unsigned(3));
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
}
