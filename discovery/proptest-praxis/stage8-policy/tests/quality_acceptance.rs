use peritus_quality_policy::{
    AcceptanceEvidence, EvidenceObservation, GateAttemptOrdinal, GateFailure, GateObservation,
    GateOutcome, ObservationKind, UnmetCondition, evaluate_acceptance,
};
use peritus_spec::{
    AcceptanceContract, CompletionPolicy, ContentReference, ContractDocuments, EvidenceRequirement,
    EvidenceRequirementId, EvidenceSource, ExportClassification, FindingSeverity, GateDefinition,
    GateExecutionPlan, GateFreshnessScope, GateGraph, GateSuccessRule, HumanApprovalPolicy,
    Requirement, RequirementId, ReviewCategory, ReviewPolicy, ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, GateExecutionId, GateId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
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
    #![proptest_config(ProptestConfig { cases: 1024, max_shrink_iters: 20_000, .. ProptestConfig::default() })]
    #[test]
    fn generated_gate_evidence_revisions(gate in 0u8..6, attempt in 1u16..=u16::MAX,
        evidence in 0u8..3) {
        check(gate, attempt, evidence);
    }
}
