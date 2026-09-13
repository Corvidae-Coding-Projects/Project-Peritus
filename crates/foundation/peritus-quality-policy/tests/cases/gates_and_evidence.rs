use crate::support::{ContractOptions, Fixture, digest};
use peritus_quality_policy::{
    AcceptanceEvidence, EvidenceObservation, GateAttemptOrdinal, GateFailure, GateObservation,
    GateOutcome, UnmetCondition, evaluate_acceptance,
};
use peritus_spec::EvidenceRequirementId;
use peritus_types::{GateExecutionId, GateId};

#[test]
fn failed_and_missing_gates_never_accept() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();
    let review = fixture.review(
        revision,
        70,
        80,
        vec![fixture.category_a, fixture.category_b],
        Vec::new(),
        130,
        true,
    );
    let failed = AcceptanceEvidence::new(
        vec![fixture.gate(revision, GateOutcome::Failed(GateFailure::PredicateFailed))],
        vec![review],
        fixture.required_evidence(&contract, revision),
        Vec::new(),
        Vec::new(),
    )
    .expect("evidence");
    let decision = evaluate_acceptance(&contract, revision, &failed);
    assert!(decision.unmet_conditions().contains(&UnmetCondition::GateDidNotPass {
        gate_id: fixture.gate_id,
        failure: GateFailure::PredicateFailed,
    }));

    let missing = AcceptanceEvidence::new(
        Vec::new(),
        vec![fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        )],
        fixture.required_evidence(&contract, revision),
        Vec::new(),
        Vec::new(),
    )
    .expect("evidence");
    assert!(
        evaluate_acceptance(&contract, revision, &missing)
            .unmet_conditions()
            .contains(&UnmetCondition::MissingGate(fixture.gate_id))
    );
}

#[test]
fn unknown_gate_and_evidence_are_explicit_rejections() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();
    let unknown_gate = GateId::new(crate::support::bytes(21)).expect("unknown gate");
    let unknown_evidence = EvidenceRequirementId::new(digest(104));
    let evidence = AcceptanceEvidence::new(
        vec![
            fixture.gate(revision, GateOutcome::Passed),
            GateObservation::new(
                GateExecutionId::new(crate::support::bytes(31)).expect("execution"),
                unknown_gate,
                GateAttemptOrdinal::new(1).expect("attempt"),
                revision,
                GateOutcome::Passed,
                digest(32),
            ),
        ],
        vec![fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        )],
        vec![
            EvidenceObservation::new(fixture.gate_evidence, revision, digest(100)),
            EvidenceObservation::new(fixture.review_evidence, revision, digest(101)),
            EvidenceObservation::new(unknown_evidence, revision, digest(104)),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("canonical evidence");
    let decision = evaluate_acceptance(&contract, revision, &evidence);
    assert!(decision.unmet_conditions().contains(&UnmetCondition::UnknownGate(unknown_gate)));
    assert!(
        decision.unmet_conditions().contains(&UnmetCondition::UnknownEvidence(unknown_evidence,))
    );
}

#[test]
fn missing_required_artifact_never_accepts() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();
    let evidence = AcceptanceEvidence::new(
        vec![fixture.gate(revision, GateOutcome::Passed)],
        vec![fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        )],
        vec![EvidenceObservation::new(fixture.gate_evidence, revision, digest(100))],
        Vec::new(),
        Vec::new(),
    )
    .expect("canonical evidence");
    assert!(
        evaluate_acceptance(&contract, revision, &evidence)
            .unmet_conditions()
            .contains(&UnmetCondition::MissingEvidence(fixture.review_evidence))
    );
}
