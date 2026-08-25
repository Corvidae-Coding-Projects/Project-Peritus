//! Focused causal-domain and projection adversarial tests.

mod support;

use peritus_projection::ProjectionState;
use peritus_trace::{
    ApplyOutcome, CausalBinding, DiagnosticCode, Observation, ObservationKind, ObservedTime,
    SafeAttribute, SafeAttributeKey, SafeAttributeValue, SpanKind, SpanOutcome, TraceErrorKind,
    TraceProjectionState,
};
use peritus_types::{AttemptId, RunId, SessionId, TurnId};

use support::{binding, event, observation, span, trace};

#[test]
fn binding_hierarchy_requires_ancestors_and_refines_them() {
    let session = SessionId::new([1; 16]).expect("session");
    let run = RunId::new([2; 16]).expect("run");
    let attempt = AttemptId::new([3; 16]).expect("attempt");
    let turn = TurnId::new([4; 16]).expect("turn");
    let root = CausalBinding::session(session).with_run(run);
    let attempt_binding = root.with_attempt(attempt).expect("attempt follows run");
    let leaf = attempt_binding.with_turn(turn).expect("turn follows attempt");

    assert!(leaf.refines(root));
    assert!(leaf.refines(attempt_binding));
    assert_eq!(
        CausalBinding::session(session).with_turn(turn).expect_err("turn without attempt").kind(),
        TraceErrorKind::InvalidBinding,
    );
}

#[test]
fn observation_rejects_noncanonical_and_self_causal_collections() {
    let trace_id = trace(10);
    let span_id = span(11);
    let attributes = vec![
        SafeAttribute::new(SafeAttributeKey::Status, SafeAttributeValue::Count(1)),
        SafeAttribute::new(SafeAttributeKey::BudgetUnits, SafeAttributeValue::Count(2)),
    ];
    let error = Observation::new(
        event(12),
        trace_id,
        span_id,
        1,
        None,
        Vec::new(),
        binding(1),
        ObservedTime::new(1, 1).expect("time"),
        ObservationKind::SpanStarted(SpanKind::AgentTurn),
        attributes,
        Vec::new(),
    )
    .expect_err("attributes are out of key order");
    assert_eq!(error.kind(), TraceErrorKind::NonCanonical);

    let error = Observation::new(
        event(12),
        trace_id,
        span_id,
        2,
        None,
        vec![event(12)],
        binding(1),
        ObservedTime::new(2, 2).expect("time"),
        ObservationKind::Diagnostic(DiagnosticCode::RecoveryStarted),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("event cannot precede itself");
    assert_eq!(error.kind(), TraceErrorKind::CausalIntegrity);
}

#[test]
fn projection_accepts_exact_replay_and_rejects_changed_duplicates() {
    let trace_id = trace(20);
    let span_id = span(21);
    let binding = binding(2);
    let start = observation(
        22,
        trace_id,
        span_id,
        1,
        None,
        Vec::new(),
        binding,
        1,
        ObservationKind::SpanStarted(SpanKind::Recovery),
    );
    let mut state = TraceProjectionState::default();
    assert_eq!(state.apply(start.clone(), 1).expect("first apply"), ApplyOutcome::Applied);
    assert_eq!(
        state.apply(start, 1).expect("same bytes and position are idempotent"),
        ApplyOutcome::ExactDuplicate,
    );

    let changed = observation(
        22,
        trace_id,
        span(23),
        1,
        None,
        Vec::new(),
        binding,
        2,
        ObservationKind::SpanStarted(SpanKind::Internal),
    );
    assert_eq!(
        state.apply(changed, 2).expect_err("same event id changed bytes").kind(),
        TraceErrorKind::DuplicateConflict,
    );
}

#[test]
fn projection_enforces_predecessor_time_parent_and_terminal_rules() {
    let trace_id = trace(30);
    let root_span = span(31);
    let child_span = span(32);
    let binding = binding(3);
    let root = observation(
        33,
        trace_id,
        root_span,
        1,
        None,
        Vec::new(),
        binding,
        10,
        ObservationKind::SpanStarted(SpanKind::AgentTurn),
    );
    let child = observation(
        34,
        trace_id,
        child_span,
        1,
        Some(root_span),
        vec![event(33)],
        binding,
        11,
        ObservationKind::SpanStarted(SpanKind::Tool),
    );
    let child_end = observation(
        35,
        trace_id,
        child_span,
        2,
        Some(root_span),
        vec![event(34)],
        binding,
        12,
        ObservationKind::SpanEnded(SpanOutcome::Ok),
    );
    let after_end = observation(
        36,
        trace_id,
        child_span,
        3,
        Some(root_span),
        vec![event(35)],
        binding,
        13,
        ObservationKind::Diagnostic(DiagnosticCode::ToolDispatchCompleted),
    );
    let mut state = TraceProjectionState::default();
    state.apply(root, 1).expect("root start");
    state.apply(child, 2).expect("causal child start");
    state.apply(child_end, 3).expect("child end");
    assert_eq!(
        state.apply(after_end, 4).expect_err("closed span cannot advance").kind(),
        TraceErrorKind::InvalidTransition,
    );

    let missing = observation(
        37,
        trace_id,
        root_span,
        2,
        None,
        vec![event(99)],
        binding,
        9,
        ObservationKind::Diagnostic(DiagnosticCode::RecoveryFailed),
    );
    assert_eq!(
        state.apply(missing, 4).expect_err("missing causal event").kind(),
        TraceErrorKind::CausalIntegrity,
    );
}

#[test]
fn rejected_first_observations_are_failure_atomic_and_do_not_bind_session() {
    let trace_id = trace(40);
    let mut state = TraceProjectionState::default();
    let genesis = state.clone();
    let genesis_bytes = state.encode();
    let genesis_digest = state.invariant_digest();

    let diagnostic_before_start = observation(
        41,
        trace_id,
        span(42),
        2,
        None,
        Vec::new(),
        binding(4),
        1,
        ObservationKind::Diagnostic(DiagnosticCode::RecoveryStarted),
    );
    assert_eq!(
        state
            .apply(diagnostic_before_start, 1)
            .expect_err("diagnostic cannot create a span")
            .kind(),
        TraceErrorKind::InvalidTransition,
    );
    assert_eq!(state.encode(), genesis_bytes);
    assert_eq!(state.invariant_digest(), genesis_digest);
    assert_eq!(state, genesis);
    assert_eq!(state.trace_count(), 0);
    assert!(state.trace(trace_id).is_none());

    let missing_parent = observation(
        43,
        trace_id,
        span(44),
        1,
        Some(span(45)),
        Vec::new(),
        binding(5),
        1,
        ObservationKind::SpanStarted(SpanKind::Tool),
    );
    assert_eq!(
        state.apply(missing_parent, 1).expect_err("parent must already exist").kind(),
        TraceErrorKind::CausalIntegrity,
    );
    assert_eq!(state.encode(), genesis_bytes);
    assert_eq!(state.invariant_digest(), genesis_digest);
    assert_eq!(state, genesis);
    assert_eq!(state.trace_count(), 0);
    assert!(state.trace(trace_id).is_none());

    let different_session = binding(6);
    let valid = observation(
        46,
        trace_id,
        span(47),
        1,
        None,
        Vec::new(),
        different_session,
        1,
        ObservationKind::SpanStarted(SpanKind::AgentTurn),
    );
    state.apply(valid, 1).expect("rejected observations bind no session");
    let snapshot = state.trace(trace_id).expect("valid trace");
    assert_eq!(snapshot.span(span(47)).expect("valid span").binding(), different_session);
    assert_eq!(state.trace_count(), 1);
    assert_eq!(state.observation_count(), 1);
}
