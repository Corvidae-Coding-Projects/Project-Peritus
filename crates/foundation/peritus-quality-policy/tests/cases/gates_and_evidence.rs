use crate::support::{ContractOptions, Fixture, digest};
use peritus_quality_policy::{
    AcceptanceEvidence, EvidenceObservation, GateAttemptOrdinal, GateFailure, GateObservation,
    GateOutcome, UnmetCondition, evaluate_acceptance,
};
use peritus_spec::EvidenceRequirementId;
use peritus_types::{GateExecutionId, GateId, Sha256Digest};

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

#[test]
fn every_required_artifact_identity_byte_must_match() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();

    for requirement in contract.evidence_requirements() {
        for byte in 0..Sha256Digest::LENGTH {
            let mut changed_bytes = requirement.id().digest().into_bytes();
            changed_bytes[byte] ^= 1;
            let changed_id = EvidenceRequirementId::new(Sha256Digest::new(changed_bytes));
            let mut artifacts = fixture.required_evidence(&contract, revision);
            for observation in &mut artifacts {
                if observation.requirement_id() == requirement.id() {
                    *observation = EvidenceObservation::new(changed_id, revision, digest(200));
                }
            }
            artifacts.sort_by_key(EvidenceObservation::requirement_id);
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
                artifacts,
                Vec::new(),
                Vec::new(),
            )
            .expect("canonical evidence with a different requirement identity");
            let decision = evaluate_acceptance(&contract, revision, &evidence);
            assert!(!decision.is_acceptable());
            assert!(
                decision
                    .unmet_conditions()
                    .contains(&UnmetCondition::MissingEvidence(requirement.id()))
            );
            assert!(
                decision.unmet_conditions().contains(&UnmetCondition::UnknownEvidence(changed_id))
            );
        }
    }
}

#[test]
fn every_required_gate_identity_byte_must_match() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();

    for byte in 0..fixture.gate_id.as_bytes().len() {
        let mut changed_bytes = fixture.gate_id.into_bytes();
        changed_bytes[byte] ^= 1;
        let changed_id = GateId::new(changed_bytes).expect("different gate identity");
        let evidence = AcceptanceEvidence::new(
            vec![GateObservation::new(
                GateExecutionId::new(crate::support::bytes(31)).expect("execution"),
                changed_id,
                GateAttemptOrdinal::new(1).expect("attempt"),
                revision,
                GateOutcome::Passed,
                digest(32),
            )],
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
        .expect("canonical evidence with a different gate identity");
        let decision = evaluate_acceptance(&contract, revision, &evidence);
        assert!(!decision.is_acceptable());
        assert!(
            decision.unmet_conditions().contains(&UnmetCondition::MissingGate(fixture.gate_id))
        );
        assert!(decision.unmet_conditions().contains(&UnmetCondition::UnknownGate(changed_id)));
    }
}

#[test]
fn each_declared_gate_requires_a_current_passing_result() {
    let fixture = Fixture::new();
    let gate_ids = [
        fixture.gate_id,
        GateId::new(crate::support::bytes(21)).expect("second gate"),
        GateId::new(crate::support::bytes(22)).expect("third gate"),
    ];
    let contract = fixture.contract_with_gate_ids(ContractOptions::basic(), &gate_ids);
    let revision = fixture.revision();

    for missing in [None, Some(0), Some(1), Some(2)] {
        let gates = gate_ids
            .iter()
            .enumerate()
            .filter(|(index, _)| Some(*index) != missing)
            .map(|(_, gate_id)| {
                GateObservation::new(
                    GateExecutionId::new(gate_id.into_bytes()).expect("execution"),
                    *gate_id,
                    GateAttemptOrdinal::new(1).expect("attempt"),
                    revision,
                    GateOutcome::Passed,
                    digest(32),
                )
            })
            .collect();
        let evidence = AcceptanceEvidence::new(
            gates,
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
        .expect("canonical gate observations");
        let decision = evaluate_acceptance(&contract, revision, &evidence);
        assert_eq!(decision.is_acceptable(), missing.is_none());
        if let Some(index) = missing {
            assert!(
                decision.unmet_conditions().contains(&UnmetCondition::MissingGate(gate_ids[index]))
            );
        }
    }
}
