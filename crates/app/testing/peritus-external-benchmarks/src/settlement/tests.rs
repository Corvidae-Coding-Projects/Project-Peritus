use super::*;

pub fn fixture_report(root: &Path) -> InvocationReport {
    InvocationReport {
        schema_version: 6,
        suite: BenchmarkSuite::TerminalBench,
        handshake: HandshakeReport {
            adapter_schema_version: 6,
            product_protocol_version: 1,
            suite_revision: "0".repeat(40),
            config_digest: "1".repeat(64),
            workspace_available: true,
            workspace: root.to_path_buf(),
            trace_path: root.join("trace"),
            evidence_path: root.join("evidence"),
            recovery_path: root.join("recovery"),
            agent_identity: identity(),
            provider_routes: vec![route("unchecked")],
        },
        agent_identity: identity(),
        success: true,
        disposition: "accepted",
        terminal_cause: "completed",
        candidate: Some(CandidateReport {
            stage: "qualified",
            digest: "5".repeat(64),
            changed_paths: vec![PathBuf::from("src/lib.rs")],
        }),
        qualification: QualificationReport::accepted(
            "gates passed".to_owned(),
            "review passed".to_owned(),
        ),
        provider_routes: vec![route("live_canary")],
        external_evaluation: ExternalEvaluation::default(),
        task_id: "task".to_owned(),
        session_id: "session".to_owned(),
        harness_model_id: "model".to_owned(),
        workspace: root.to_path_buf(),
        baseline_head: None,
        initialized_repository: false,
        created_artifact_manifest: false,
        writer: "writer".to_owned(),
        reviewer: "reviewer".to_owned(),
        elapsed_ms: 0,
        trace_path: root.join("trace"),
        conversation_turn: 1,
        session_trace_paths: Vec::new(),
        usage_proxy: None,
        projected_responses: 0,
        usage: TraceUsage::default(),
        resources: ResourceReport::default(),
        last_observation_path: None,
        relocatable_paths: None,
        summary: None,
        changed_paths: vec![PathBuf::from("src/lib.rs")],
        failure_kind: None,
        failure: None,
    }
}

fn seed(root: &Path) -> ReportSeed {
    ReportSeed {
        suite: BenchmarkSuite::TerminalBench,
        handshake: HandshakeReport {
            adapter_schema_version: 6,
            product_protocol_version: 1,
            suite_revision: "0".repeat(40),
            config_digest: "1".repeat(64),
            workspace_available: true,
            workspace: root.to_path_buf(),
            trace_path: root.join("trace"),
            evidence_path: root.join("evidence"),
            recovery_path: root.join("recovery"),
            agent_identity: identity(),
            provider_routes: vec![route("unchecked")],
        },
        agent_identity: identity(),
        task_id: "task".to_owned(),
        session_id: "session".to_owned(),
        harness_model_id: "model".to_owned(),
        workspace: root.to_path_buf(),
        trace_path: root.join("trace"),
        conversation_turn: 1,
        writer: "openai/writer".to_owned(),
        reviewer: "anthropic/reviewer".to_owned(),
        run_id: RunId::new([3; 16]).expect("run"),
        workspace_id: WorkspaceId::new([4; 16]).expect("workspace"),
        baseline: None,
        provider_routes: Vec::new(),
        session_trace_paths: Vec::new(),
        usage_proxy: None,
        projected_responses: 0,
        usage: TraceUsage::default(),
        resources: ResourceReport::default(),
        last_observation_path: None,
        relocatable_paths: None,
    }
}

fn identity() -> BenchmarkAgentIdentity {
    BenchmarkAgentIdentity {
        package_version: "0.0.0",
        source_revision: Some("0123456789abcdef0123456789abcdef01234567"),
        binary_sha256: "2".repeat(64),
    }
}

fn route(availability: &'static str) -> ProviderRouteReport {
    ProviderRouteReport {
        role: "writer",
        provider: "openai".to_owned(),
        model: "fixture-model".to_owned(),
        route: "account_runtime",
        availability,
        text: true,
        image_input: true,
        maximum_context_tokens: 200_000,
        tool_protocol: true,
    }
}

fn snapshot(paths: Vec<PathBuf>) -> CandidateSnapshot {
    CandidateSnapshot { digest: "5".repeat(64), digest_bytes: [5; 32], changed_paths: paths }
}

#[test]
fn only_qualified_candidate_is_native_success() {
    let root = tempfile::tempdir().expect("root");
    let mut accepted = build_report(
        &seed(root.path()),
        Instant::now(),
        TerminalFacts {
            cause: SettlementCause::Completed,
            snapshot: Some(snapshot(vec![PathBuf::from("src/lib.rs")])),
            qualified: true,
            qualification: QualificationReport::accepted(
                "gates passed".to_owned(),
                "review passed".to_owned(),
            ),
            summary: Some("done".to_owned()),
            failure_kind: None,
            failure: None,
        },
    )
    .expect("accepted report");
    assert!(accepted.success);
    assert_eq!(accepted.disposition, "accepted");
    assert!(accepted.external_evaluation.reward.is_none());

    accepted.external_evaluation.reward = Some(0.0);
    assert!(accepted.success);
    assert_eq!(accepted.disposition, "accepted");

    let mut candidate = build_report(
        &seed(root.path()),
        Instant::now(),
        TerminalFacts {
            cause: SettlementCause::Provider,
            snapshot: Some(snapshot(vec![PathBuf::from("src/lib.rs")])),
            qualified: false,
            qualification: QualificationReport::candidate("changed", None, None),
            summary: None,
            failure_kind: Some("provider".to_owned()),
            failure: Some("capacity".to_owned()),
        },
    )
    .expect("candidate report");
    assert!(!candidate.success);
    assert_eq!(candidate.disposition, "candidate_available");
    assert!(candidate.external_evaluation.reward.is_none());

    candidate.external_evaluation.reward = Some(1.0);
    assert!(!candidate.success);
    assert_eq!(candidate.disposition, "candidate_available");
}

#[test]
fn cancellation_and_deadline_remain_distinct_terminal_causes() {
    let root = tempfile::tempdir().expect("root");
    for (cause, disposition, terminal) in [
        (SettlementCause::Cancellation, "cancelled", "cancellation"),
        (SettlementCause::Deadline, "failed_no_candidate", "deadline"),
    ] {
        let report = build_report(
            &seed(root.path()),
            Instant::now(),
            TerminalFacts {
                cause,
                snapshot: None,
                qualified: false,
                qualification: QualificationReport::missing(),
                summary: None,
                failure_kind: Some(terminal.to_owned()),
                failure: Some("fixture".to_owned()),
            },
        )
        .expect("terminal report");
        assert_eq!(report.disposition, disposition);
        assert_eq!(report.terminal_cause, terminal);
        assert!(!report.success);
    }
}

#[test]
fn provider_failure_publishes_one_parseable_terminal_report() {
    let root = tempfile::tempdir().expect("root");
    let report_publisher = AtomicPublisher::prepare(
        &root.path().join("evidence"),
        &root.path().join("recovery"),
        "provider-failure".to_owned(),
    )
    .expect("publisher");
    let mut guard = InvocationGuard::new(seed(root.path()), report_publisher);
    let error = BenchmarkError::Provider("provider unavailable".to_owned());

    let report = guard.fail(SettlementCause::Provider, &error).expect("terminal report");

    assert_eq!(report.terminal_cause, "provider");
    assert_eq!(report.failure_kind.as_deref(), Some("provider"));
    assert!(!report.success);
    let document: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("evidence/invocation.json")).expect("published report"),
    )
    .expect("parse report");
    assert_eq!(document["terminal_cause"], "provider");
    assert_eq!(document["failure_kind"], "provider");
}

#[test]
fn dropping_an_unsettled_guard_publishes_internal_invariant_report() {
    let root = tempfile::tempdir().expect("root");
    let report_publisher = AtomicPublisher::prepare(
        &root.path().join("evidence"),
        &root.path().join("recovery"),
        "scope-exit".to_owned(),
    )
    .expect("publisher");

    drop(InvocationGuard::new(seed(root.path()), report_publisher));

    let document: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("evidence/invocation.json")).expect("published report"),
    )
    .expect("parse report");
    assert_eq!(document["disposition"], "failed_no_candidate");
    assert_eq!(document["terminal_cause"], "internal_invariant");
    assert_eq!(document["success"], false);
}
