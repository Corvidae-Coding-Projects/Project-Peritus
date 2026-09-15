use super::*;
use crate::publication::AtomicPublisher;
use crate::settlement::InvocationGuard;
use peritus_model_protocol::{EventEnvelope, ModelEvent, ProtocolLimits, encode_event_envelope};
use peritus_product_runner::DeveloperTraceFrameKind;

#[test]
fn run_identity_is_stable_and_task_scoped() {
    assert_eq!(run_id("session", "task").unwrap(), run_id("session", "task").unwrap());
    assert_ne!(run_id("session", "task").unwrap(), run_id("session", "other").unwrap());
}

#[test]
fn interrupted_trace_retains_completed_evidence_and_native_deadline_cause() {
    let root = tempfile::tempdir().expect("root");
    let mut guard = guard(root.path());
    let path = guard.seed().trace_path.clone();
    write_interrupted_trace(&path);
    let retained = retain_harness_evidence(
        &mut guard,
        &[(path, "bounded task".to_owned())],
        Some(root.path()),
    );
    let error = BenchmarkError::Workspace("native elapsed deadline reached".to_owned());
    let facts = crate::settlement::TerminalFacts::failure(
        SettlementCause::Deadline,
        Some(snapshot()),
        &error,
    );

    let report = settle_retained_facts(&mut guard, facts, snapshot(), retained)
        .expect("published native failure");

    assert!(!report.success);
    assert_eq!(report.terminal_cause, "deadline");
    assert_eq!(report.failure_kind.as_deref(), Some("workspace"));
    assert_eq!(report.projected_responses, 1);
    assert_eq!(report.usage.requests, 1);
    assert_eq!(report.resources.model_requests, 2);
    assert_eq!(
        report.candidate.expect("partial candidate").changed_paths,
        vec![PathBuf::from("output.txt")]
    );
    let failure = report.failure.expect("failure detail");
    assert!(failure.contains("native elapsed deadline reached"));
    assert!(failure.contains("provider response has no terminal event"));
}

#[test]
fn accepted_native_facts_do_not_accept_an_incomplete_trace() {
    let root = tempfile::tempdir().expect("root");
    let mut guard = guard(root.path());
    let path = guard.seed().trace_path.clone();
    write_interrupted_trace(&path);
    let retained = retain_harness_evidence(
        &mut guard,
        &[(path, "bounded task".to_owned())],
        Some(root.path()),
    );
    let facts = crate::settlement::TerminalFacts {
        cause: SettlementCause::Completed,
        snapshot: Some(snapshot()),
        qualified: true,
        qualification: QualificationReport::accepted(
            "gates passed".to_owned(),
            "review passed".to_owned(),
        ),
        summary: Some("native completion".to_owned()),
        failure_kind: None,
        failure: None,
    };

    let report = settle_retained_facts(&mut guard, facts, snapshot(), retained)
        .expect("published inconsistent-trace failure");

    assert!(!report.success);
    assert_eq!(report.terminal_cause, "adapter");
    assert_eq!(report.failure_kind.as_deref(), Some("trace"));
    assert_eq!(report.projected_responses, 1);
}

#[test]
fn malformed_trace_remains_an_adapter_failure() {
    let root = tempfile::tempdir().expect("root");
    let mut guard = guard(root.path());
    let path = guard.seed().trace_path.clone();
    std::fs::write(&path, [9_u8, 0, 0, 0, 0, 0, 0, 0, 0]).expect("unknown trace tag");
    let retained = retain_harness_evidence(
        &mut guard,
        &[(path, "bounded task".to_owned())],
        Some(root.path()),
    );
    let facts = crate::settlement::TerminalFacts::failure(
        SettlementCause::Deadline,
        Some(snapshot()),
        &BenchmarkError::Workspace("native elapsed deadline reached".to_owned()),
    );

    let report = settle_retained_facts(&mut guard, facts, snapshot(), retained)
        .expect("published corrupt-trace failure");

    assert!(!report.success);
    assert_eq!(report.terminal_cause, "adapter");
    assert_eq!(report.failure_kind.as_deref(), Some("trace"));
}

fn guard(root: &Path) -> InvocationGuard {
    let mut seed = crate::settlement::tests::seed(root);
    seed.suite = BenchmarkSuite::HarnessBench;
    seed.usage_proxy = Some(root.join("usage-proxy"));
    seed.resources.model_requests = 2;
    let publisher = AtomicPublisher::prepare(
        &root.join("evidence"),
        &root.join("recovery"),
        "interrupted-trace".to_owned(),
    )
    .expect("publisher");
    InvocationGuard::new(seed, publisher)
}

fn snapshot() -> candidate::CandidateSnapshot {
    candidate::CandidateSnapshot {
        digest: "33".repeat(32),
        digest_bytes: [0x33; 32],
        changed_paths: vec![PathBuf::from("output.txt")],
    }
}

fn write_interrupted_trace(path: &Path) {
    let events = [
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ResponseCompleted,
        ModelEvent::ResponseStarted { response_id: None, model: None },
    ];
    let mut bytes = Vec::new();
    for (index, event) in events.into_iter().enumerate() {
        let sequence = u64::try_from(index + 1).expect("sequence");
        let digest = u8::try_from(sequence).expect("digest");
        let envelope = EventEnvelope::new(
            sequence,
            None,
            None,
            peritus_types::Sha256Digest::new([digest; 32]),
            event,
        )
        .expect("envelope");
        let payload =
            encode_event_envelope(&envelope, ProtocolLimits::PRODUCTION).expect("encoded envelope");
        bytes.push(DeveloperTraceFrameKind::ProviderEnvelope.tag());
        bytes.extend_from_slice(&u64::try_from(payload.len()).expect("length").to_le_bytes());
        bytes.extend_from_slice(&payload);
    }
    std::fs::write(path, bytes).expect("interrupted trace");
}
