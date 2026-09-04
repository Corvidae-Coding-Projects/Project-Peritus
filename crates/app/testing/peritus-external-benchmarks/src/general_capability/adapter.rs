use std::{fs, time::Duration};

use peritus_run_settlement::SettlementCause;
use serde::Deserialize;

use crate::{
    BenchmarkError, admission,
    args::TerminalBenchInput,
    identity::INVOCATION_REPORT_SCHEMA_VERSION,
    publication::{AtomicPublisher, PublicationReceipt},
    settlement::{InvocationGuard, TerminalFacts},
    trace,
};

use super::fixture::{Expected, FixtureSet};

const CASES: &str = include_str!("../../tests/fixtures/general-capability/adapter/cases.json");

#[derive(Deserialize)]
struct Case {
    name: String,
    expected: Expected,
}

#[test]
fn adapter_admission_timeout_trace_and_publication_fail_closed() {
    let fixtures: FixtureSet<Case> = serde_json::from_str(CASES).expect("adapter fixtures");
    assert_case_shape(&fixtures.cases);
    stale_schema_and_missing_workspace_are_rejected();
    absent_trace_parent_is_not_invented();
    deadline_is_published_as_terminal_truth();
    publication_has_primary_recovery_and_total_failure_outcomes();
}

fn stale_schema_and_missing_workspace_are_rejected() {
    let root = tempfile::tempdir().expect("adapter root");
    let stale = terminal_input(root.path(), INVOCATION_REPORT_SCHEMA_VERSION - 1);
    assert!(matches!(
        admission::terminalbench(stale),
        Err(BenchmarkError::UnsupportedSchema { .. })
    ));

    let missing = terminal_input(&root.path().join("missing"), INVOCATION_REPORT_SCHEMA_VERSION);
    assert!(matches!(
        admission::terminalbench(missing),
        Err(BenchmarkError::MissingWorkspace { .. })
    ));
}

fn absent_trace_parent_is_not_invented() {
    let root = tempfile::tempdir().expect("trace root");
    let parent = root.path().join("absent");
    let trace_path = parent.join("run.trace");
    let error = trace::prepare(&trace_path).expect_err("absent trace parent");
    assert_eq!(error.stable_kind(), "trace");
    fs::create_dir(&parent).expect("trace parent");
    trace::prepare(&trace_path).expect("prepared trace");
    assert!(trace_path.is_file());
}

fn deadline_is_published_as_terminal_truth() {
    let root = tempfile::tempdir().expect("deadline root");
    let publisher = AtomicPublisher::prepare(
        &root.path().join("evidence"),
        &root.path().join("recovery"),
        "deadline".to_owned(),
    )
    .expect("deadline publisher");
    let mut guard = InvocationGuard::new(crate::settlement::tests::seed(root.path()), publisher);
    let report = guard
        .finalize(TerminalFacts {
            cause: SettlementCause::Deadline,
            snapshot: None,
            qualified: false,
            qualification: crate::evidence::QualificationReport::missing(),
            summary: None,
            failure_kind: Some("deadline".to_owned()),
            failure: Some("generic deadline elapsed".to_owned()),
        })
        .expect("deadline report");
    assert!(!report.success);
    assert_eq!(report.disposition, "failed_no_candidate");
    assert_eq!(report.terminal_cause, "deadline");
}

fn publication_has_primary_recovery_and_total_failure_outcomes() {
    let primary_root = tempfile::tempdir().expect("primary root");
    let mut primary = AtomicPublisher::prepare(
        &primary_root.path().join("evidence"),
        &primary_root.path().join("recovery"),
        "primary".to_owned(),
    )
    .expect("primary publisher");
    let mut report = crate::settlement::tests::fixture_report(primary_root.path());
    assert!(matches!(
        primary.publish(&mut report).expect("primary publication"),
        PublicationReceipt::Primary(_)
    ));
    assert!(matches!(primary.publish(&mut report), Err(BenchmarkError::DuplicateFinalization)));

    let recovery_root = tempfile::tempdir().expect("recovery root");
    let recovery_evidence = recovery_root.path().join("evidence");
    let mut recovery = AtomicPublisher::prepare(
        &recovery_evidence,
        &recovery_root.path().join("recovery"),
        "recovered".to_owned(),
    )
    .expect("recovery publisher");
    fs::create_dir(recovery_evidence.join("invocation.json")).expect("block primary");
    let mut report = crate::settlement::tests::fixture_report(recovery_root.path());
    assert!(matches!(
        recovery.publish(&mut report).expect("recovery publication"),
        PublicationReceipt::Recovery(_)
    ));
    assert!(!report.success);
    assert_eq!(report.disposition, "recovery_required");

    let failed_root = tempfile::tempdir().expect("failed root");
    let evidence = failed_root.path().join("evidence");
    let recovery_dir = failed_root.path().join("recovery");
    let mut failed = AtomicPublisher::prepare(&evidence, &recovery_dir, "failed".to_owned())
        .expect("failed publisher");
    fs::create_dir(evidence.join("invocation.json")).expect("block failed primary");
    fs::create_dir(recovery_dir.join("failed.recovery.json")).expect("block failed recovery");
    let mut report = crate::settlement::tests::fixture_report(failed_root.path());
    assert!(matches!(failed.publish(&mut report), Err(BenchmarkError::ReportPublication { .. })));
}

fn terminal_input(root: &std::path::Path, schema: u32) -> TerminalBenchInput {
    TerminalBenchInput {
        workspace: root.to_path_buf(),
        evidence_dir: root.join("evidence"),
        prompt_file: root.join("prompt.txt"),
        session_id: "generic-session".to_owned(),
        task_id: "generic-task".to_owned(),
        model_id: "generic-model".to_owned(),
        max_elapsed: Duration::from_secs(30),
        adapter_schema_version: schema,
        suite_revision: "a".repeat(40),
    }
}

fn assert_case_shape(cases: &[Case]) {
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].expected, Expected::Success);
    assert_eq!(cases[1].expected, Expected::Partial);
    assert_eq!(cases[2].expected, Expected::Failure);
    assert!(cases.iter().all(|case| !case.name.is_empty()));
}
