use crate::test_support as support;
use crate::{
    GateArtifact, GateAttemptResult, GateCommandKind, GateErrorKind, GateOutcomeKind,
    GateRejection, RecoveryRequirement, RetryPermission,
};

#[test]
fn zero_requirement_receipt_is_bound_to_the_exact_result_event_and_manifest() {
    let fixture = support::fixture(1);
    let (state, attempt) = evidence_pending(&fixture, support::passing(fixture.first, 161), 160);
    let publication = publication(&fixture, &state, attempt, 900, Vec::new());
    let receipt = publication.receipt_from_records(Vec::new()).expect("zero-evidence receipt");
    assert!(!publication.manifest_bytes().is_empty());
    assert_eq!(receipt.manifest_digest(), publication.manifest_digest());

    let valid = support::command(
        &state,
        164,
        GateCommandKind::PublishEvidence {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            receipt,
        },
    );
    assert!(crate::decide(&fixture.plan, &state, &valid).is_ok());

    let result = state.slot(fixture.first).expect("slot").last_result().expect("result");
    let wrong_publication = crate::EvidencePublication::new(
        fixture.run_id,
        fixture.first,
        attempt,
        fixture.revision,
        peritus_types::EventId::new([199; 16]).expect("event id"),
        900,
        result.tool_result_digest(),
        Vec::new(),
        Vec::new(),
    )
    .expect("wrong publication");
    assert_ne!(wrong_publication.manifest_digest(), publication.manifest_digest());
    let wrong_receipt =
        wrong_publication.receipt_from_records(Vec::new()).expect("bound wrong receipt");
    let command = support::command(
        &state,
        165,
        GateCommandKind::PublishEvidence {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            receipt: wrong_receipt,
        },
    );
    let before = state.clone();
    let error = crate::decide(&fixture.plan, &state, &command).expect_err("wrong result event");
    assert_eq!(error.kind(), GateErrorKind::Rejected(GateRejection::EvidenceInvalid));
    assert_eq!(state, before);
}

#[test]
fn receipt_with_another_quality_artifact_set_is_rejected_without_mutation() {
    let fixture = support::fixture(1);
    let artifact = GateArtifact::from_parts(
        support::digest(170),
        12,
        "application/octet-stream".to_owned(),
        "stdout".to_owned(),
    )
    .expect("artifact");
    let result = GateAttemptResult::from_parts(
        fixture.first,
        GateOutcomeKind::Passed,
        support::digest(171),
        Some(support::digest(172)),
        Some(support::digest(173)),
        Some(peritus_types::ProcessId::new([174; 16]).expect("process")),
        vec![artifact],
        RetryPermission::Never,
        RecoveryRequirement::None,
    )
    .expect("passing result");
    let (state, attempt) = evidence_pending(&fixture, result, 170);
    let unrelated = publication(&fixture, &state, attempt, 910, Vec::new());
    let receipt = unrelated.receipt_from_records(Vec::new()).expect("unrelated receipt");
    let command = support::command(
        &state,
        175,
        GateCommandKind::PublishEvidence {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            receipt,
        },
    );
    let before = state.clone();
    let error = crate::decide(&fixture.plan, &state, &command).expect_err("artifact mismatch");
    assert_eq!(error.kind(), GateErrorKind::Rejected(GateRejection::EvidenceInvalid));
    assert_eq!(state, before);
}

fn evidence_pending(
    fixture: &support::Fixture,
    result: GateAttemptResult,
    seed: u8,
) -> (crate::GateRunState, crate::ActiveAttempt) {
    let started =
        crate::start(&fixture.plan, &support::start_command(fixture, seed)).expect("start");
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();
    let attempt = support::attempt(fixture, seed.wrapping_add(1), 1);
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        seed.wrapping_add(1),
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        seed.wrapping_add(2),
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        seed.wrapping_add(3),
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            result,
        },
    );
    (state, attempt)
}

fn publication(
    fixture: &support::Fixture,
    state: &crate::GateRunState,
    attempt: crate::ActiveAttempt,
    position: u64,
    artifacts: Vec<GateArtifact>,
) -> crate::EvidencePublication {
    let result = state.slot(fixture.first).expect("slot").last_result().expect("result");
    let result_event =
        state.slot(fixture.first).expect("slot").result_event().expect("result event");
    crate::EvidencePublication::new(
        fixture.run_id,
        fixture.first,
        attempt,
        fixture.revision,
        result_event,
        position,
        result.tool_result_digest(),
        Vec::new(),
        artifacts,
    )
    .expect("publication")
}
