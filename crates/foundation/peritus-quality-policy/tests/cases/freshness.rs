use crate::support::{ContractOptions, Fixture};
use peritus_quality_policy::{
    AcceptanceEvidence, ObservationKind, UnmetCondition, evaluate_acceptance,
};

#[test]
fn one_field_revision_drift_is_stale_for_every_tuple_dimension() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let observed = fixture.revision();
    let requested_cases = [
        fixture.revision_from([6, 2, 3, 1, 1, 4, 5]),
        fixture.revision_from([1, 6, 3, 1, 1, 4, 5]),
        fixture.revision_from([1, 2, 6, 1, 1, 4, 5]),
        fixture.revision_from([1, 2, 3, 2, 1, 4, 5]),
        fixture.revision_from([1, 2, 3, 1, 2, 4, 5]),
        fixture.revision_from([1, 2, 3, 1, 1, 6, 5]),
        fixture.revision_from([1, 2, 3, 1, 1, 4, 6]),
    ];
    for requested in requested_cases {
        let evidence = fixture.evidence_set(
            &contract,
            observed,
            vec![fixture.review(
                observed,
                70,
                80,
                vec![fixture.category_a, fixture.category_b],
                Vec::new(),
                130,
                true,
            )],
            Vec::new(),
            Vec::new(),
        );
        let decision = evaluate_acceptance(&contract, requested, &evidence);
        assert!(!decision.is_acceptable());
        for kind in [ObservationKind::Gate, ObservationKind::Review, ObservationKind::Evidence] {
            assert!(decision.unmet_conditions().iter().any(|condition| matches!(
                condition,
                UnmetCondition::StaleObservation { kind: actual, .. } if *actual == kind
            )));
        }
    }
}

#[test]
fn stale_observations_do_not_satisfy_missing_current_requirements() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let stale = fixture.revision_from([1, 2, 3, 1, 2, 4, 5]);
    let current = fixture.revision();
    let evidence = AcceptanceEvidence::new(
        vec![fixture.gate(stale, peritus_quality_policy::GateOutcome::Passed)],
        vec![fixture.review(
            stale,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        )],
        fixture.required_evidence(&contract, stale),
        Vec::new(),
        Vec::new(),
    )
    .expect("stale but canonical evidence");
    let decision = evaluate_acceptance(&contract, current, &evidence);
    assert!(decision.unmet_conditions().contains(&UnmetCondition::MissingGate(fixture.gate_id)));
    assert!(
        decision
            .unmet_conditions()
            .contains(&UnmetCondition::MissingEvidence(fixture.gate_evidence,))
    );
    assert!(
        decision.unmet_conditions().iter().any(|condition| matches!(
            condition,
            UnmetCondition::ReviewerQuorum { observed: 0, .. }
        ))
    );
}
