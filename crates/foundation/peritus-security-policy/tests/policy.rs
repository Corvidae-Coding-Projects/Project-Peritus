//! H0 exact-candidate, completeness, review, and blocker policy tests.

use peritus_security_policy::{
    AcceptanceCriterion, ArtifactObservation, CriterionObservation, EvidenceArtifactKind,
    FindingLifecycle, FindingObservation, FindingSeverity, IndependentSecurityReview,
    IntegratedCandidate, InventoryKind, InventoryObservation, RequirementObservation,
    ReviewCompletion, ReviewScope, ReviewerIdentity, SECURITY_QUALIFICATION_PROBE_COUNT,
    SecurityControlOutcome, SecurityEvidence, SecurityQualificationAdmission,
    SecurityQualificationOutcome, SecurityRequirement, SecurityVerdict, UnmetSecurityCondition,
    evaluate_security_readiness,
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
fn every_native_probe_outcome_is_required_for_final_admission() {
    let candidate = candidate(17);
    let decision =
        evaluate_security_readiness(candidate, &complete_evidence(candidate, Vec::new()));
    let complete = [SecurityQualificationOutcome::Passed; SECURITY_QUALIFICATION_PROBE_COUNT];
    assert!(SecurityQualificationAdmission::evaluate(&decision, &complete).is_ready());

    for index in 0..SECURITY_QUALIFICATION_PROBE_COUNT {
        let mut failed = complete;
        failed[index] = SecurityQualificationOutcome::Failed;
        assert!(!SecurityQualificationAdmission::evaluate(&decision, &failed).is_ready());
    }
}

#[test]
fn every_required_security_dimension_fails_closed_when_removed() {
    let candidate = candidate(7);

    for requirement in SecurityRequirement::ALL {
        let evidence = evidence_except(
            candidate,
            Some(MissingDimension::Requirement(requirement)),
            Vec::new(),
        );
        let decision = evaluate_security_readiness(candidate, &evidence);
        assert!(!decision.is_ready());
        assert!(
            decision
                .unmet_conditions()
                .contains(&UnmetSecurityCondition::MissingRequirement(requirement))
        );
    }

    for criterion in AcceptanceCriterion::ALL {
        let evidence =
            evidence_except(candidate, Some(MissingDimension::Criterion(criterion)), Vec::new());
        let decision = evaluate_security_readiness(candidate, &evidence);
        assert!(!decision.is_ready());
        assert!(
            decision
                .unmet_conditions()
                .contains(&UnmetSecurityCondition::MissingCriterion(criterion))
        );
    }

    for kind in InventoryKind::ALL {
        let evidence =
            evidence_except(candidate, Some(MissingDimension::Inventory(kind)), Vec::new());
        let decision = evaluate_security_readiness(candidate, &evidence);
        assert!(!decision.is_ready());
        assert!(
            decision.unmet_conditions().contains(&UnmetSecurityCondition::MissingInventory(kind))
        );
    }

    for kind in EvidenceArtifactKind::ALL {
        let evidence =
            evidence_except(candidate, Some(MissingDimension::Artifact(kind)), Vec::new());
        let decision = evaluate_security_readiness(candidate, &evidence);
        assert!(!decision.is_ready());
        assert!(
            decision
                .unmet_conditions()
                .contains(&UnmetSecurityCondition::MissingEvidenceArtifact(kind))
        );
    }
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
    evidence_except(candidate, None, findings)
}

#[derive(Clone, Copy)]
enum MissingDimension {
    Requirement(SecurityRequirement),
    Criterion(AcceptanceCriterion),
    Inventory(InventoryKind),
    Artifact(EvidenceArtifactKind),
}

fn evidence_except(
    candidate: IntegratedCandidate,
    missing: Option<MissingDimension>,
    findings: Vec<FindingObservation>,
) -> SecurityEvidence {
    let requirements = SecurityRequirement::ALL
        .into_iter()
        .filter(|requirement| {
            !matches!(missing, Some(MissingDimension::Requirement(value)) if value == *requirement)
        })
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
        .filter(|criterion| {
            !matches!(missing, Some(MissingDimension::Criterion(value)) if value == *criterion)
        })
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
        .filter(
            |kind| !matches!(missing, Some(MissingDimension::Inventory(value)) if value == *kind),
        )
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
        .filter(
            |kind| !matches!(missing, Some(MissingDimension::Artifact(value)) if value == *kind),
        )
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
