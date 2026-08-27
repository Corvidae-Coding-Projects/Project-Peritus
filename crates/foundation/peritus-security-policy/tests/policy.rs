//! H0 exact-candidate, completeness, review, and blocker policy tests.

use peritus_security_policy::{
    AcceptanceCriterion, ArtifactObservation, CriterionObservation, EvidenceArtifactKind,
    FindingLifecycle, FindingObservation, FindingSeverity, IndependentSecurityReview,
    IntegratedCandidate, InventoryKind, InventoryObservation, RequirementObservation,
    ReviewCompletion, ReviewScope, ReviewerIdentity, SecurityControlOutcome, SecurityEvidence,
    SecurityRequirement, SecurityVerdict, UnmetSecurityCondition, evaluate_security_readiness,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, FindingId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

#[test]
fn complete_exact_candidate_is_security_ready_without_release_authority() {
    let candidate = candidate(1);
    let evidence = complete_evidence(candidate, Vec::new());
    let decision = evaluate_security_readiness(candidate, &evidence);
    assert_eq!(decision.verdict(), SecurityVerdict::Ready);
    assert!(decision.is_ready());
    assert!(decision.unmet_conditions().is_empty());
}

#[test]
fn any_candidate_mutation_invalidates_prior_security_evidence() {
    let original = candidate(1);
    let mutated = candidate(2);
    let evidence = complete_evidence(original, Vec::new());
    let decision = evaluate_security_readiness(mutated, &evidence);
    assert_eq!(decision.verdict(), SecurityVerdict::NotReady);
    assert!(decision.unmet_conditions().iter().any(|condition| {
        matches!(condition, UnmetSecurityCondition::CandidateMismatch { .. })
    }));
}

#[test]
fn missing_independent_review_is_not_ready() {
    let candidate = candidate(3);
    let mut evidence = complete_evidence(candidate, Vec::new());
    evidence = SecurityEvidence::new(
        evidence.requirements().to_vec(),
        evidence.criteria().to_vec(),
        evidence.inventories().to_vec(),
        evidence.artifacts().to_vec(),
        None,
    )
    .expect("canonical evidence");
    let decision = evaluate_security_readiness(candidate, &evidence);
    assert!(decision.unmet_conditions().contains(&UnmetSecurityCondition::MissingExternalReview));
    assert!(!decision.is_ready());
}

#[test]
fn unresolved_high_finding_blocks_readiness() {
    let candidate = candidate(4);
    let finding = FindingObservation::new(
        FindingId::new([9; 16]).expect("finding"),
        candidate,
        FindingSeverity::High,
        FindingLifecycle::AcceptedRisk { authority_digest: digest(90) },
    );
    let evidence = complete_evidence(candidate, vec![finding]);
    let decision = evaluate_security_readiness(candidate, &evidence);
    assert!(decision.unmet_conditions().iter().any(|condition| {
        matches!(condition, UnmetSecurityCondition::UnresolvedReleaseBlocker { .. })
    }));
    assert!(!decision.is_ready());
}

#[test]
fn resolved_high_finding_requires_remediation_and_retest_evidence() {
    let candidate = candidate(5);
    let finding = FindingObservation::new(
        FindingId::new([8; 16]).expect("finding"),
        candidate,
        FindingSeverity::Critical,
        FindingLifecycle::Resolved { remediation_digest: digest(91), retest_digest: digest(92) },
    );
    let evidence = complete_evidence(candidate, vec![finding]);
    assert!(evaluate_security_readiness(candidate, &evidence).is_ready());
}

fn complete_evidence(
    candidate: IntegratedCandidate,
    findings: Vec<FindingObservation>,
) -> SecurityEvidence {
    let requirements = SecurityRequirement::ALL
        .into_iter()
        .enumerate()
        .map(|(index, requirement)| {
            RequirementObservation::new(
                requirement,
                candidate,
                SecurityControlOutcome::Passed,
                digest(u8::try_from(index + 10).expect("small index")),
            )
        })
        .collect();
    let criteria = AcceptanceCriterion::ALL
        .into_iter()
        .enumerate()
        .map(|(index, criterion)| {
            CriterionObservation::new(
                criterion,
                candidate,
                SecurityControlOutcome::Passed,
                digest(u8::try_from(index + 30).expect("small index")),
            )
        })
        .collect();
    let inventories = InventoryKind::ALL
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            InventoryObservation::new(
                kind,
                candidate,
                true,
                digest(u8::try_from(index + 50).expect("small index")),
            )
        })
        .collect();
    let artifacts = EvidenceArtifactKind::ALL
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            ArtifactObservation::new(
                kind,
                candidate,
                digest(u8::try_from(index + 60).expect("small index")),
            )
        })
        .collect();
    let review = IndependentSecurityReview::new(
        candidate,
        ReviewerIdentity::new(ActorId::new([2; 16]).expect("reviewer"), digest(2), digest(3)),
        ActorId::new([1; 16]).expect("producer"),
        digest(1),
        ReviewCompletion::Completed,
        ReviewScope::ALL.to_vec(),
        digest(4),
        findings,
    )
    .expect("review");
    SecurityEvidence::new(requirements, criteria, inventories, artifacts, Some(review))
        .expect("evidence")
}

fn candidate(seed: u8) -> IntegratedCandidate {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([seed; 16]).expect("acceptance"),
        HarnessId::new([seed.wrapping_add(1); 16]).expect("harness"),
        WorkspaceId::new([seed.wrapping_add(2); 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([seed.wrapping_add(3); 16]).expect("policy"),
        ProviderProfileId::new([seed.wrapping_add(4); 16]).expect("provider"),
    );
    IntegratedCandidate::new(revision, digest(seed), digest(seed + 10), digest(seed + 20))
}

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}
