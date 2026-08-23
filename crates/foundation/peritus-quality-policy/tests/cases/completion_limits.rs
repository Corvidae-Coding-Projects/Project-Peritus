use crate::support::{ContractOptions, Fixture};
use peritus_quality_policy::{
    AcceptanceEvidence, GateAttemptOrdinal, GateOutcome, ReviewCycleOrdinal, UnmetCondition,
    evaluate_acceptance,
};

#[test]
fn observation_ordinals_are_one_based() {
    assert!(GateAttemptOrdinal::new(0).is_err());
    assert!(ReviewCycleOrdinal::new(0).is_err());
    assert_eq!(GateAttemptOrdinal::new(1).expect("first attempt").get(), 1);
    assert_eq!(ReviewCycleOrdinal::new(1).expect("first cycle").get(), 1);
}

#[test]
fn gate_attempt_at_limit_passes_and_next_attempt_is_rejected() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.max_gate_attempts = 3;
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let review = || {
        fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        )
    };
    let evidence_with_gate = |attempt| {
        AcceptanceEvidence::new(
            vec![fixture.gate_at(revision, GateOutcome::Passed, attempt)],
            vec![review()],
            fixture.required_evidence(&contract, revision),
            Vec::new(),
            Vec::new(),
        )
        .expect("evidence")
    };

    let boundary = evidence_with_gate(3);
    assert!(evaluate_acceptance(&contract, revision, &boundary).is_acceptable());

    let exceeded = evidence_with_gate(4);
    let decision = evaluate_acceptance(&contract, revision, &exceeded);
    assert_eq!(
        decision.unmet_conditions(),
        &[UnmetCondition::GateAttemptLimitExceeded {
            gate_id: fixture.gate_id,
            attempt: 4,
            maximum: 3,
        }]
    );
}

#[test]
fn review_cycle_at_limit_passes_and_next_cycle_is_rejected() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.max_review_cycles = 4;
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let evidence_at_cycle = |cycle_ordinal| {
        let review = fixture.review_at_cycle(
            revision,
            70,
            cycle_ordinal,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        );
        fixture.evidence_set(&contract, revision, vec![review], Vec::new(), Vec::new())
    };

    let boundary = evidence_at_cycle(4);
    assert!(evaluate_acceptance(&contract, revision, &boundary).is_acceptable());

    let exceeded = evidence_at_cycle(5);
    let decision = evaluate_acceptance(&contract, revision, &exceeded);
    assert_eq!(
        decision.unmet_conditions(),
        &[UnmetCondition::ReviewCycleLimitExceeded {
            cycle_id: exceeded.reviews()[0].cycle_id(),
            cycle: 5,
            maximum: 4,
        }]
    );
}

#[test]
fn maximum_u16_limits_do_not_overflow() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.max_gate_attempts = u16::MAX;
    options.max_review_cycles = u16::MAX;
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let review = fixture.review_at_cycle(
        revision,
        70,
        u16::MAX,
        80,
        vec![fixture.category_a, fixture.category_b],
        Vec::new(),
        130,
        true,
    );
    let evidence = AcceptanceEvidence::new(
        vec![fixture.gate_at(revision, GateOutcome::Passed, u16::MAX)],
        vec![review],
        fixture.required_evidence(&contract, revision),
        Vec::new(),
        Vec::new(),
    )
    .expect("evidence");

    assert!(evaluate_acceptance(&contract, revision, &evidence).is_acceptable());
}
