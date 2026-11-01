use peritus_quality_policy::{
    AcceptanceEvidence, EvidenceObservation, FindingDisposition, FindingObservation,
    GateAttemptOrdinal, GateFailure, GateObservation, GateOutcome, ObservationKind,
    ReviewCycleOrdinal, ReviewObservation, ReviewerIdentity, UnmetCondition, evaluate_acceptance,
};
use peritus_spec::{
    AcceptanceContract, CompletionPolicy, ContentReference, ContractDocuments, EvidenceRequirement,
    EvidenceRequirementId, EvidenceSource, ExportClassification, FindingSeverity, GateDefinition,
    GateExecutionPlan, GateFreshnessScope, GateGraph, GateSuccessRule, HumanApprovalPolicy,
    Requirement, RequirementId, ReviewCategory, ReviewPolicy, ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, EnvironmentId, FindingId, GateExecutionId, GateId, Generation,
    HarnessId, PolicyId, ProviderProfileId, ReviewCycleId, RevisionNumber, RevisionTuple,
    Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;

fn d(n: u8) -> Sha256Digest {
    Sha256Digest::new([n; 32])
}
fn reference(n: u8) -> ContentReference {
    ContentReference::new(d(n))
}
fn gate_id() -> GateId {
    GateId::new([7; 16]).unwrap()
}
fn evidence_id() -> EvidenceRequirementId {
    EvidenceRequirementId::new(d(8))
}

fn revision(number: u64) -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).unwrap(),
        HarnessId::new([2; 16]).unwrap(),
        WorkspaceId::new([3; 16]).unwrap(),
        Generation::first(),
        RevisionNumber::new(number).unwrap(),
        PolicyId::new([4; 16]).unwrap(),
        ProviderProfileId::new([5; 16]).unwrap(),
    )
}

fn contract() -> AcceptanceContract {
    let gate = GateDefinition::new(
        gate_id(),
        GateExecutionPlan::new(
            reference(11),
            EnvironmentId::new([6; 16]).unwrap(),
            reference(12),
            reference(13),
            GateSuccessRule::ExitCodeZero,
            1_000,
            reference(14),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .unwrap(),
        vec![],
        vec![evidence_id()],
    )
    .unwrap();
    AcceptanceContract::new(
        AcceptanceSpecId::new([1; 16]).unwrap(),
        d(15),
        ContractDocuments::new(
            reference(16),
            reference(17),
            reference(18),
            reference(19),
            reference(20),
            reference(21),
            reference(22),
            reference(23),
        ),
        vec![Requirement::new(RequirementId::new(d(24)), reference(25))],
        vec![],
        vec![],
        GateGraph::new(vec![gate]).unwrap(),
        ReviewPolicy::new(
            vec![ReviewCategory::new(d(26))],
            1,
            ReviewerIndependence::new(false, false, false, false, false, false),
            FindingSeverity::High,
        )
        .unwrap(),
        vec![EvidenceRequirement::new(
            evidence_id(),
            reference(27),
            EvidenceSource::Gate(gate_id()),
            ExportClassification::Internal,
        )],
        CompletionPolicy::new(2, 1).unwrap(),
        HumanApprovalPolicy::NotRequired,
        WaiverPolicy::Forbidden,
    )
    .unwrap()
}

fn gate_observation(kind: u8, attempt: u16) -> Option<GateObservation> {
    if kind == 0 {
        return None;
    }
    Some(GateObservation::new(
        GateExecutionId::new([9; 16]).unwrap(),
        gate_id(),
        GateAttemptOrdinal::new(attempt).unwrap(),
        revision(if kind >= 4 { 2 } else { 1 }),
        if kind == 2 || kind == 5 {
            GateOutcome::Failed(GateFailure::UnsuccessfulExit)
        } else {
            GateOutcome::Passed
        },
        d(28),
    ))
}

fn evidence_observation(kind: u8) -> Option<EvidenceObservation> {
    if kind == 0 {
        return None;
    }
    Some(EvidenceObservation::new(evidence_id(), revision(if kind == 2 { 2 } else { 1 }), d(29)))
}

fn has(conditions: &[UnmetCondition], needle: UnmetCondition) -> bool {
    conditions.contains(&needle)
}

fn check(gate_kind: u8, attempt: u16, evidence_kind: u8) {
    let gate = gate_observation(gate_kind, attempt);
    let artifact = evidence_observation(evidence_kind);
    let result = evaluate_acceptance(
        &contract(),
        revision(1),
        &AcceptanceEvidence::new(
            gate.into_iter().collect(),
            vec![],
            artifact.into_iter().collect(),
            vec![],
            vec![],
        )
        .unwrap(),
    );
    let unmet = result.unmet_conditions();
    assert_eq!(result.gate_attempt_limit(), 2);
    assert_eq!(result.review_cycle_limit(), 1);
    assert!(!result.is_acceptable()); // The required review is absent in this bounded family.
    assert_eq!(
        has(unmet, UnmetCondition::MissingGate(gate_id())),
        gate_kind == 0 || gate_kind >= 4,
        "gate={gate_kind} attempt={attempt}"
    );
    assert_eq!(
        has(unmet, UnmetCondition::StaleObservation { kind: ObservationKind::Gate, index: 0 }),
        gate_kind >= 4,
        "gate={gate_kind} attempt={attempt}"
    );
    assert_eq!(
        has(
            unmet,
            UnmetCondition::GateDidNotPass {
                gate_id: gate_id(),
                failure: GateFailure::UnsuccessfulExit,
            }
        ),
        gate_kind == 2,
        "gate={gate_kind} attempt={attempt}"
    );
    assert_eq!(
        has(
            unmet,
            UnmetCondition::GateAttemptLimitExceeded { gate_id: gate_id(), attempt, maximum: 2 }
        ),
        (gate_kind == 1 || gate_kind == 3) && attempt > 2,
        "gate={gate_kind} attempt={attempt}"
    );
    assert_eq!(
        has(unmet, UnmetCondition::MissingEvidence(evidence_id())),
        evidence_kind != 1,
        "evidence={evidence_kind}"
    );
    assert_eq!(
        has(unmet, UnmetCondition::StaleObservation { kind: ObservationKind::Evidence, index: 0 }),
        evidence_kind == 2,
        "evidence={evidence_kind}"
    );
}

#[test]
fn fixed_gate_and_artifact_boundaries() {
    for gate in 0..=5 {
        for attempt in 1..=4 {
            for evidence in 0..=2 {
                check(gate, attempt, evidence);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))), .. ProptestConfig::default() })]
    #[test]
    fn generated_gate_evidence_revisions(gate in 0u8..6, attempt in 1u16..=u16::MAX,
        evidence in 0u8..3) {
        check(gate, attempt, evidence);
    }
}

// Positive fixtures prevent an always-rejecting evaluator from passing this suite.
// Each defect is independently selectable: a stale artifact must remain observable
// even alongside a failed gate, a missing review, or an unresolved finding.
#[derive(Clone, Debug)]
struct AcceptanceCase {
    gate: u8,
    artifact: u8,
    review: u8,
    attempt: u16,
    cycle: u16,
    severity: u8,
    disposition: u8,
    revision_dimension: u8,
}

fn changed_revision(dimension: u8) -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([if dimension == 0 { 31 } else { 1 }; 16]).unwrap(),
        HarnessId::new([if dimension == 1 { 32 } else { 2 }; 16]).unwrap(),
        WorkspaceId::new([if dimension == 2 { 33 } else { 3 }; 16]).unwrap(),
        Generation::new(if dimension == 3 { 2 } else { 1 }).unwrap(),
        RevisionNumber::new(if dimension == 4 { 2 } else { 1 }).unwrap(),
        PolicyId::new([if dimension == 5 { 34 } else { 4 }; 16]).unwrap(),
        ProviderProfileId::new([if dimension == 6 { 35 } else { 5 }; 16]).unwrap(),
    )
}

fn check_complete(case: &AcceptanceCase) {
    let current = revision(1);
    let stale = changed_revision(case.revision_dimension);
    let gates = if case.gate == 0 {
        vec![]
    } else {
        vec![GateObservation::new(
            GateExecutionId::new([9; 16]).unwrap(),
            gate_id(),
            GateAttemptOrdinal::new(case.attempt).unwrap(),
            if case.gate == 3 { stale } else { current },
            if case.gate == 2 {
                GateOutcome::Failed(GateFailure::UnsuccessfulExit)
            } else {
                GateOutcome::Passed
            },
            d(28),
        )]
    };
    let artifacts = if case.artifact == 0 {
        vec![]
    } else {
        vec![EvidenceObservation::new(
            evidence_id(),
            if case.artifact == 2 { stale } else { current },
            d(29),
        )]
    };
    let severity = [
        FindingSeverity::Advisory,
        FindingSeverity::Low,
        FindingSeverity::Medium,
        FindingSeverity::High,
        FindingSeverity::Critical,
    ][usize::from(case.severity)];
    let reviewed_revision = if case.review == 2 { stale } else { current };
    let findings = if case.disposition == 0 {
        vec![]
    } else {
        vec![FindingObservation::new(
            FindingId::new([41; 16]).unwrap(),
            severity,
            match case.disposition {
                1 => FindingDisposition::Open,
                2 => FindingDisposition::Resolved {
                    revision: reviewed_revision,
                    evidence_digest: d(42),
                },
                3 => FindingDisposition::WaiverRequested,
                _ => unreachable!(),
            },
            d(43),
        )]
    };
    let reviews = if case.review == 0 {
        vec![]
    } else {
        vec![
            ReviewObservation::new(
                ReviewCycleId::new([44; 16]).unwrap(),
                ReviewCycleOrdinal::new(case.cycle).unwrap(),
                reviewed_revision,
                ReviewerIdentity::new(
                    ActorId::new([45; 16]).unwrap(),
                    d(46),
                    d(47),
                    d(48),
                    d(49),
                    d(50),
                    true,
                ),
                vec![ReviewCategory::new(d(if case.review == 3 { 51 } else { 26 }))],
                findings,
                d(52),
            )
            .unwrap(),
        ]
    };
    let evidence = AcceptanceEvidence::new(gates, reviews, artifacts, vec![], vec![]).unwrap();
    let actual = evaluate_acceptance(&contract(), current, &evidence);
    let blocked = case.severity >= 3 && matches!(case.disposition, 1 | 3);
    let expected = case.gate == 1
        && case.attempt <= 2
        && case.artifact == 1
        && case.review == 1
        && case.cycle <= 1
        && !blocked;
    assert_eq!(
        actual.is_acceptable(),
        expected,
        "case={case:?} unmet={:?}",
        actual.unmet_conditions()
    );
    assert_eq!(actual.unmet_conditions().is_empty(), expected, "case={case:?}");
    for (kind, is_stale) in [
        (ObservationKind::Gate, case.gate == 3),
        (ObservationKind::Evidence, case.artifact == 2),
        (ObservationKind::Review, case.review == 2),
    ] {
        assert_eq!(
            has(actual.unmet_conditions(), UnmetCondition::StaleObservation { kind, index: 0 }),
            is_stale,
            "case={case:?}"
        );
    }
    assert_eq!(
        actual
            .unmet_conditions()
            .iter()
            .any(|condition| matches!(condition, UnmetCondition::UnwaivedBlocker { .. })),
        matches!(case.review, 1 | 3) && blocked,
        "case={case:?}"
    );
}

fn complete_case() -> AcceptanceCase {
    AcceptanceCase {
        gate: 1,
        artifact: 1,
        review: 1,
        attempt: 1,
        cycle: 1,
        severity: 3,
        disposition: 0,
        revision_dimension: 0,
    }
}

#[test]
fn complete_evidence_and_every_revision_dimension() {
    check_complete(&complete_case());
    for dimension in 0..7 {
        for field in 0..3 {
            let mut case = complete_case();
            case.revision_dimension = dimension;
            match field {
                0 => case.gate = 3,
                1 => case.artifact = 2,
                _ => case.review = 2,
            }
            check_complete(&case);
        }
    }
    for severity in 0..5 {
        for disposition in 0..4 {
            check_complete(&AcceptanceCase { severity, disposition, ..complete_case() });
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))), .. ProptestConfig::default() })]
    #[test]
    fn complete_acceptance_with_interacting_defects(
        gate in 0u8..4, artifact in 0u8..3, review in 0u8..4,
        attempt in prop_oneof![Just(1u16), Just(2), Just(3), Just(u16::MAX)],
        cycle in prop_oneof![Just(1u16), Just(2), Just(u16::MAX)],
        severity in 0u8..5, disposition in 0u8..4, revision_dimension in 0u8..7,
    ) {
        check_complete(&AcceptanceCase { gate, artifact, review, attempt, cycle,
            severity, disposition, revision_dimension });
    }
}
