//! Real-process fault receipts and redaction regressions.

use peritus_conformance::{Observation, ObservationValue, ProviderFailureKind, ProviderScenario};
use peritus_model_protocol::FailureCategory;

use super::super::diagnostics::ProbeError;
use super::super::observations;
use super::{FakeExecutable, Probe, profile, request};

fn value<'a>(evidence: &'a [Observation], id: &str) -> &'a ObservationValue {
    evidence.iter().find(|fact| fact.id().as_str() == id).expect("fact is present").value()
}

fn text(evidence: &[Observation], id: &str) -> String {
    let ObservationValue::Text(value) = value(evidence, id) else {
        panic!("expected text fact {id}");
    };
    value.as_str().to_owned()
}

#[test]
fn obstructed_trace_preserves_observation_failure_and_cleanup_after_real_auth_process() {
    let scenario = ProviderScenario::AuthenticationFailure;
    let profile = profile(scenario, 0xe2).unwrap();
    let request = request(&profile, false, None).unwrap();
    let helper = FakeExecutable::install(scenario).unwrap();
    let directory = helper.directory.path().to_owned();
    std::fs::create_dir(helper.trace_path()).unwrap();
    let Err(error) = Probe::run_installed(scenario, profile, request, helper) else {
        panic!("a blocked trace cannot qualify as observed authentication rejection");
    };
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "trace.read");
    assert_eq!(text(&evidence, "probe.terminal"), "failed");
    assert_eq!(text(&evidence, "probe.failure-category"), "Authentication");
    assert_eq!(value(&evidence, "probe.trace-readable"), &ObservationValue::Boolean(false));
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
    assert!(
        !evidence.iter().any(|fact| fact.id().as_str() == "probe.auth-requests"),
        "an unreadable trace must not invent a zero auth request count"
    );
    assert!(!directory.exists(), "cleanup runs after the failed observation");
    assert!(!format!("{evidence:?}").contains(&directory.to_string_lossy().into_owned()));
}

#[test]
fn wrong_terminal_keeps_actual_category_counts_and_original_failed_predicate() {
    let scenario = ProviderScenario::UsageAccounting;
    let profile = profile(scenario, 0xe3).unwrap();
    let request = request(&profile, false, None).unwrap();
    let probe = Probe::run_request(scenario, profile, request).unwrap();
    let error = observations::failure(
        &probe,
        FailureCategory::Authentication,
        ProviderFailureKind::Authentication,
        0,
    )
    .expect_err("success cannot masquerade as authentication rejection");
    let evidence = error.into_observations();
    assert_eq!(text(&evidence, "probe.stage"), "verify.failure-category");
    assert_eq!(text(&evidence, "probe.terminal"), "completed");
    assert_eq!(text(&evidence, "probe.failure-category"), "absent");
    assert_eq!(value(&evidence, "probe.auth-requests"), &ObservationValue::Unsigned(1));
    assert_eq!(value(&evidence, "probe.turn-requests"), &ObservationValue::Unsigned(1));
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(true));
}

#[test]
fn fake_trace_failure_has_distinct_exit_and_only_redacted_io_diagnostics() {
    let helper = FakeExecutable::install(ProviderScenario::AuthenticationFailure).unwrap();
    std::fs::create_dir(helper.trace_path()).unwrap();
    let output =
        std::process::Command::new(helper.path()).args(["login", "status"]).output().unwrap();
    assert_eq!(output.status.code(), Some(91), "fixture fault is distinct from scripted exit 1");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("codex-fixture trace.read "), "{stderr}");
    assert!(!stderr.contains(&helper.directory.path().to_string_lossy().into_owned()));
    let directory = helper.directory.path().to_owned();
    let _ = helper.finish(Err::<(), _>(ProbeError::stage("test.obstructed")), Vec::new());
    assert!(!directory.exists());
}

#[test]
fn io_and_cleanup_evidence_excludes_untrusted_error_text() {
    let canary = "peritus-provider-secret-canary";
    let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, canary);
    let mut failure = ProbeError::io("trace.read", &error);
    failure.add_cleanup(&Err(error));
    let evidence = failure.into_observations();
    assert_eq!(text(&evidence, "probe.io-kind"), "PermissionDenied");
    assert_eq!(text(&evidence, "probe.cleanup-io-kind"), "PermissionDenied");
    assert_eq!(value(&evidence, "probe.directory-removed"), &ObservationValue::Boolean(false));
    assert!(!format!("{evidence:?}").contains(canary));
}

#[cfg(target_os = "linux")]
#[test]
fn immutable_fixture_alias_executes_while_a_legacy_copy_has_a_live_writer() {
    use std::os::unix::fs::MetadataExt as _;

    if !std::path::Path::new("copy-writer-child.marker").is_file() {
        // Own the injected writer in a separate process. Other parallel tests must not inherit
        // it through their forks and invalidate the assertion about its final release below.
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("copy-writer-child.marker"), b"owned").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "codex_runtime_conformance::support::tests::immutable_fixture_alias_executes_while_a_legacy_copy_has_a_live_writer",
                "--test-threads=1",
                "--nocapture",
            ])
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }

    let legacy_directory = tempfile::tempdir().unwrap();
    let legacy = legacy_directory.path().join("codex-authentication");
    std::fs::copy(super::HELPER, &legacy).unwrap();
    // A writable handle retained by a fork has the same kernel exclusion as this owned handle.
    // Holding it makes the old copy/exec failure deterministic without sleeps or spawn retries.
    let writer = std::fs::OpenOptions::new().write(true).open(&legacy).unwrap();
    let error = std::process::Command::new(&legacy)
        .args(["login", "status"])
        .output()
        .expect_err("Linux rejects execution while another owner can write the executable");
    assert_eq!(error.kind(), std::io::ErrorKind::ExecutableFileBusy);

    let helper = FakeExecutable::install(ProviderScenario::AuthenticationFailure).unwrap();
    let original = std::fs::metadata(super::HELPER).unwrap();
    let alias = std::fs::metadata(helper.path()).unwrap();
    assert_eq!(
        (alias.dev(), alias.ino()),
        (original.dev(), original.ino()),
        "fixture identity is the immutable built artifact, not newly written executable bytes"
    );
    let output =
        std::process::Command::new(helper.path()).args(["login", "status"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "the scripted auth rejection actually executes");
    assert_eq!(super::read_trace(&helper.trace_path()).unwrap(), ["auth"]);
    let directory = helper.directory.path().to_owned();
    helper.finish(Ok(()), Vec::new()).unwrap();
    assert!(!directory.exists(), "private alias is removed after child exit");
    assert!(std::path::Path::new(super::HELPER).is_file(), "cleanup retains the Cargo artifact");
    drop(writer);
    let output = std::process::Command::new(&legacy).args(["login", "status"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "releasing the writer removes the old failure");
}
