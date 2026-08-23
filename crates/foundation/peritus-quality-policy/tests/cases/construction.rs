use crate::support::{ContractOptions, Fixture, bytes, digest};
use peritus_quality_policy::{
    AcceptanceEvidence, CanonicalEvidenceCollection, EvidenceErrorKind, FindingDisposition,
    FindingObservation, GateOutcome, ReviewCycleOrdinal,
};
use peritus_spec::FindingSeverity;
use peritus_types::{ActorId, FindingId};

#[test]
fn duplicate_and_descending_gate_observations_are_rejected() {
    let fixture = Fixture::new();
    let revision = fixture.revision();
    let first = fixture.gate(revision, GateOutcome::Passed);
    let duplicate = fixture
        .gate(revision, GateOutcome::Failed(peritus_quality_policy::GateFailure::PredicateFailed));
    let error = AcceptanceEvidence::new(
        vec![first, duplicate],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("duplicate gate must fail");
    assert_eq!(error.kind(), EvidenceErrorKind::DuplicateObservation);
    assert_eq!(error.collection(), CanonicalEvidenceCollection::Gates);
}

#[test]
fn duplicate_finding_across_reviews_is_rejected() {
    let fixture = Fixture::new();
    let revision = fixture.revision();
    let id = FindingId::new(bytes(50)).expect("finding");
    let finding =
        || FindingObservation::new(id, FindingSeverity::High, FindingDisposition::Open, digest(51));
    let reviews = vec![
        fixture.review(revision, 70, 80, vec![fixture.category_a], vec![finding()], 130, true),
        fixture.review(revision, 71, 81, vec![fixture.category_b], vec![finding()], 140, true),
    ];
    let error = AcceptanceEvidence::new(Vec::new(), reviews, Vec::new(), Vec::new(), Vec::new())
        .expect_err("duplicate finding must fail");
    assert_eq!(error.collection(), CanonicalEvidenceCollection::Findings);
}

#[test]
fn review_cycle_ordinals_are_unique_and_canonical() {
    let fixture = Fixture::new();
    let revision = fixture.revision();
    let review = |cycle, ordinal, actor| {
        fixture.review_at_cycle(
            revision,
            cycle,
            ordinal,
            actor,
            vec![fixture.category_a],
            Vec::new(),
            actor,
            true,
        )
    };

    let duplicate = AcceptanceEvidence::new(
        Vec::new(),
        vec![review(70, 1, 80), review(71, 1, 81)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("one cycle ordinal cannot identify two observations");
    assert_eq!(duplicate.kind(), EvidenceErrorKind::DuplicateObservation);
    assert_eq!(duplicate.collection(), CanonicalEvidenceCollection::Reviews);

    let descending = AcceptanceEvidence::new(
        Vec::new(),
        vec![review(70, 2, 80), review(71, 1, 81)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("cycle ordinals must ascend with canonical cycle identities");
    assert_eq!(descending.kind(), EvidenceErrorKind::NonCanonicalOrder);
    assert_eq!(descending.collection(), CanonicalEvidenceCollection::Reviews);
}

#[test]
fn resolution_for_another_revision_is_rejected() {
    let fixture = Fixture::new();
    let current = fixture.revision();
    let stale = fixture.revision_from([1, 2, 3, 1, 2, 4, 5]);
    let result = peritus_quality_policy::ReviewObservation::new(
        peritus_types::ReviewCycleId::new(bytes(70)).expect("cycle"),
        ReviewCycleOrdinal::new(1).expect("cycle ordinal"),
        current,
        peritus_quality_policy::ReviewerIdentity::new(
            ActorId::new(bytes(80)).expect("actor"),
            digest(130),
            digest(131),
            digest(132),
            digest(133),
            digest(134),
            true,
        ),
        vec![fixture.category_a],
        vec![FindingObservation::new(
            FindingId::new(bytes(50)).expect("finding"),
            FindingSeverity::Critical,
            FindingDisposition::Resolved { revision: stale, evidence_digest: digest(52) },
            digest(51),
        )],
        digest(135),
    );
    let error = result.expect_err("stale resolution must fail");
    assert_eq!(error.kind(), EvidenceErrorKind::ResolutionRevisionMismatch);
}

#[test]
fn fixture_contract_remains_checked() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    assert_eq!(contract.id(), fixture.acceptance_id);
}
