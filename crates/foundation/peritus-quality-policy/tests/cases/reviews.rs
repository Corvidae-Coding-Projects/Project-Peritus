use crate::support::{ContractOptions, Fixture, bytes, digest, finding_id};
use peritus_quality_policy::{
    FindingDisposition, FindingObservation, ReviewCycleOrdinal, ReviewObservation,
    ReviewerIdentity, ReviewerIndependenceFailure, UnmetCondition, evaluate_acceptance,
};
use peritus_spec::{FindingSeverity, ReviewCategory, ReviewerIndependence};
use peritus_types::{ActorId, ReviewCycleId, Sha256Digest};

#[test]
fn missing_category_and_quorum_are_reported_together() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.quorum = 2;
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let evidence = fixture.evidence_set(
        &contract,
        revision,
        vec![fixture.review(revision, 70, 80, vec![fixture.category_a], Vec::new(), 130, true)],
        Vec::new(),
        Vec::new(),
    );
    let decision = evaluate_acceptance(&contract, revision, &evidence);
    assert!(
        decision
            .unmet_conditions()
            .contains(&UnmetCondition::MissingReviewCategory(fixture.category_b,))
    );
    assert!(
        decision
            .unmet_conditions()
            .contains(&UnmetCondition::ReviewerQuorum { required: 2, observed: 1 })
    );
}

#[test]
fn every_configured_reviewer_independence_dimension_is_enforced() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.quorum = 2;
    options.independence = ReviewerIndependence::new(true, true, true, true, true, true);
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let shared = digest(150);
    let review = |cycle: u8, actor: u8, independent: bool| {
        ReviewObservation::new(
            ReviewCycleId::new(bytes(cycle)).expect("cycle"),
            ReviewCycleOrdinal::new(u16::from(
                cycle.checked_sub(69).expect("fixture cycle starts at 70"),
            ))
            .expect("cycle ordinal"),
            revision,
            ReviewerIdentity::new(
                ActorId::new(bytes(actor)).expect("actor"),
                shared,
                shared,
                digest(actor),
                shared,
                shared,
                independent,
            ),
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            digest(cycle),
        )
        .expect("review")
    };
    let evidence = fixture.evidence_set(
        &contract,
        revision,
        vec![review(70, 80, false), review(71, 80, true)],
        Vec::new(),
        Vec::new(),
    );
    let decision = evaluate_acceptance(&contract, revision, &evidence);
    for failure in [
        ReviewerIndependenceFailure::DistinctReviewers,
        ReviewerIndependenceFailure::ProducerIndependence,
        ReviewerIndependenceFailure::DistinctContexts,
        ReviewerIndependenceFailure::DistinctModelFamilies,
        ReviewerIndependenceFailure::DistinctProviders,
        ReviewerIndependenceFailure::SharedAncestry,
    ] {
        assert!(
            decision.unmet_conditions().contains(&UnmetCondition::ReviewerIndependence(failure))
        );
    }
}

#[test]
fn reviewer_actor_reuse_follows_the_contract_flag() {
    let fixture = Fixture::new();
    let revision = fixture.revision();
    let review = |cycle: u8| {
        ReviewObservation::new(
            ReviewCycleId::new(bytes(cycle)).expect("cycle"),
            ReviewCycleOrdinal::new(u16::from(
                cycle.checked_sub(69).expect("fixture cycle starts at 70"),
            ))
            .expect("cycle ordinal"),
            revision,
            ReviewerIdentity::new(
                ActorId::new(bytes(80)).expect("shared actor"),
                digest(cycle),
                digest(cycle.wrapping_add(1)),
                digest(cycle.wrapping_add(2)),
                digest(cycle.wrapping_add(3)),
                digest(cycle.wrapping_add(4)),
                true,
            ),
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            digest(cycle.wrapping_add(5)),
        )
        .expect("review")
    };

    let mut permissive_options = ContractOptions::basic();
    permissive_options.quorum = 2;
    permissive_options.independence =
        ReviewerIndependence::new(false, true, false, false, false, false);
    let permissive = fixture.contract(permissive_options);
    let evidence = fixture.evidence_set(
        &permissive,
        revision,
        vec![review(70), review(71)],
        Vec::new(),
        Vec::new(),
    );
    assert!(evaluate_acceptance(&permissive, revision, &evidence).is_acceptable());

    let mut strict_options = permissive_options;
    strict_options.independence = ReviewerIndependence::new(true, true, false, false, false, false);
    let strict = fixture.contract(strict_options);
    let strict_evidence = fixture.evidence_set(
        &strict,
        revision,
        vec![review(70), review(71)],
        Vec::new(),
        Vec::new(),
    );
    assert!(evaluate_acceptance(&strict, revision, &strict_evidence).unmet_conditions().contains(
        &UnmetCondition::ReviewerIndependence(ReviewerIndependenceFailure::DistinctReviewers,)
    ));
}

#[test]
fn open_blocker_rejects_but_current_resolution_satisfies_blocker_policy() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();
    let blocker_id = finding_id(50);
    let finding = |disposition| {
        FindingObservation::new(blocker_id, FindingSeverity::High, disposition, digest(51))
    };
    let open = fixture.evidence_set(
        &contract,
        revision,
        vec![fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            vec![finding(FindingDisposition::Open)],
            130,
            true,
        )],
        Vec::new(),
        Vec::new(),
    );
    assert!(evaluate_acceptance(&contract, revision, &open).unmet_conditions().contains(
        &UnmetCondition::UnwaivedBlocker {
            finding_id: blocker_id,
            severity: FindingSeverity::High,
        }
    ));

    let resolved = fixture.evidence_set(
        &contract,
        revision,
        vec![fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            vec![finding(FindingDisposition::Resolved { revision, evidence_digest: digest(52) })],
            130,
            true,
        )],
        Vec::new(),
        Vec::new(),
    );
    assert!(evaluate_acceptance(&contract, revision, &resolved).is_acceptable());
}

#[test]
fn every_required_review_category_identity_byte_must_match() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();

    for byte in 0..Sha256Digest::LENGTH {
        let mut changed_bytes = fixture.category_a.digest().into_bytes();
        changed_bytes[byte] ^= 1;
        let changed = ReviewCategory::new(Sha256Digest::new(changed_bytes));
        let mut categories = vec![changed, fixture.category_b];
        categories.sort();
        let evidence = fixture.evidence_set(
            &contract,
            revision,
            vec![fixture.review(revision, 70, 80, categories, Vec::new(), 130, true)],
            Vec::new(),
            Vec::new(),
        );
        let decision = evaluate_acceptance(&contract, revision, &evidence);
        assert!(!decision.is_acceptable());
        assert!(
            decision.unmet_conditions().contains(&UnmetCondition::UnknownReviewCategory(changed))
        );
        assert!(
            decision
                .unmet_conditions()
                .contains(&UnmetCondition::MissingReviewCategory(fixture.category_a))
        );
    }
}

#[test]
fn independence_uses_all_actor_and_provenance_identity_bytes() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.quorum = 2;
    options.independence = ReviewerIndependence::new(true, true, true, true, true, true);
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let first_actor_bytes = [0x10; 16];
    let first_provenance_bytes = [0xA5; Sha256Digest::LENGTH];

    for byte in 0..Sha256Digest::LENGTH {
        let mut second_actor_bytes = first_actor_bytes;
        second_actor_bytes[byte % first_actor_bytes.len()] ^= 1;
        let mut second_provenance_bytes = first_provenance_bytes;
        second_provenance_bytes[byte] ^= 1;
        let reviews = [
            (70, 1, first_actor_bytes, first_provenance_bytes, fixture.category_a),
            (71, 2, second_actor_bytes, second_provenance_bytes, fixture.category_b),
        ]
        .into_iter()
        .map(|(cycle, ordinal, actor_bytes, provenance_bytes, category)| {
            let provenance = Sha256Digest::new(provenance_bytes);
            ReviewObservation::new(
                ReviewCycleId::new(bytes(cycle)).expect("cycle"),
                ReviewCycleOrdinal::new(ordinal).expect("ordinal"),
                revision,
                ReviewerIdentity::new(
                    ActorId::new(actor_bytes).expect("actor"),
                    provenance,
                    provenance,
                    digest(200),
                    provenance,
                    provenance,
                    true,
                ),
                vec![category],
                Vec::new(),
                digest(cycle),
            )
            .expect("review")
        })
        .collect();
        let evidence = fixture.evidence_set(&contract, revision, reviews, Vec::new(), Vec::new());
        let decision = evaluate_acceptance(&contract, revision, &evidence);
        assert!(
            decision.is_acceptable(),
            "distinct identity byte {byte}: {:?}",
            decision.unmet_conditions()
        );
    }
}

#[test]
fn every_current_review_participates_in_independence_checks() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.independence = ReviewerIndependence::new(true, true, true, false, false, false);
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let reviews = [(70, 80, 130), (71, 81, 140), (72, 82, 140)]
        .into_iter()
        .map(|(cycle, actor, provenance)| {
            fixture.review(
                revision,
                cycle,
                actor,
                vec![fixture.category_a, fixture.category_b],
                Vec::new(),
                provenance,
                true,
            )
        })
        .collect();
    let evidence = fixture.evidence_set(&contract, revision, reviews, Vec::new(), Vec::new());
    let decision = evaluate_acceptance(&contract, revision, &evidence);
    assert!(!decision.is_acceptable());
    assert_eq!(
        decision.unmet_conditions(),
        &[UnmetCondition::ReviewerIndependence(ReviewerIndependenceFailure::DistinctContexts)]
    );
}
