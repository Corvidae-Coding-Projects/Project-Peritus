//! Complete exact evidence produces the sole H4 Ready path.

mod support;

use peritus_release_policy::{
    Diagnostic, PRODUCTION_CRITERIA, RELEASE_QUALIFICATION_CHECK_COUNT, REQUIRED_EVIDENCE,
    ReleaseQualificationAdmission, ReleaseQualificationCheck, ReleaseVerdict,
};
use support::ready_inputs;

#[test]
fn complete_exact_evidence_is_ready_without_publication_authority() {
    let decision = ready_inputs().evaluate();
    assert!(decision.is_ready());
    assert_eq!(decision.verdict(), ReleaseVerdict::Ready);
    assert!(decision.diagnostics().is_empty());
    assert!(
        decision.criteria().iter().all(peritus_release_policy::CriterionAssessment::is_satisfied)
    );
    assert!(
        decision.evidence().iter().all(peritus_release_policy::EvidenceAssessment::is_satisfied)
    );
    assert!(
        decision
            .qualifications()
            .iter()
            .all(peritus_release_policy::QualificationAssessment::is_satisfied)
    );
    assert!(decision.reviews().is_satisfied());
    assert!(decision.findings().is_satisfied());

    for (assessment, definition) in decision.criteria().iter().zip(PRODUCTION_CRITERIA) {
        assert_eq!(assessment.criterion(), definition.criterion());
        assert!(assessment.is_satisfied());
    }
    for (assessment, requirement) in decision.evidence().iter().zip(REQUIRED_EVIDENCE) {
        assert_eq!(assessment.requirement(), requirement);
        assert_eq!(assessment.contributing_count(), 1);
        assert_eq!(assessment.contributing_digest(), support::digest(requirement.stable_id() + 20));
        assert_eq!(assessment.stale_count(), 0);
        assert_eq!(assessment.mismatched_count(), 0);
        assert_eq!(assessment.wrong_source_count(), 0);
        assert_eq!(assessment.unreviewed_count(), 0);
        assert_eq!(assessment.unsigned_count(), 0);
        assert!(!assessment.is_conflicting());
    }
    for assessment in decision.qualifications() {
        assert_eq!(assessment.ready_count(), 1);
        assert_eq!(assessment.stale_count(), 0);
        assert_eq!(assessment.mismatched_count(), 0);
        assert_eq!(assessment.unreviewed_count(), 0);
        assert_eq!(assessment.not_ready_count(), 0);
        assert!(!assessment.is_conflicting());
        assert_eq!(assessment.report_digest(), support::digest(140 + assessment.slice().ordinal()),);
    }
    let reviews = decision.reviews();
    assert_eq!(reviews.approved_count(), 2);
    assert_eq!(reviews.stale_count(), 0);
    assert_eq!(reviews.mismatched_count(), 0);
    assert_eq!(reviews.changes_required_count(), 0);
    assert_eq!(reviews.self_review_count(), 0);
    assert_eq!(reviews.non_independent_count(), 0);
    assert!(!reviews.has_duplicate_reviewer());
    assert!(!reviews.has_shared_context());
    assert!(!reviews.has_conflicting_review());
    let findings = decision.findings();
    assert_eq!(findings.stale_count(), 0);
    assert_eq!(findings.mismatched_count(), 0);
    assert_eq!(findings.open_count(), 0);
    assert_eq!(findings.release_blocking_count(), 0);
    assert_eq!(findings.ignored_count(), 0);
    assert_eq!(findings.quarantined_count(), 0);
    assert_eq!(findings.invalid_waiver_count(), 0);
    assert!(!findings.has_conflicting_finding());
}

#[test]
fn every_required_artifact_fails_closed_when_removed() {
    for requirement in REQUIRED_EVIDENCE {
        let mut inputs = ready_inputs();
        inputs.observations.retain(|value| value.requirement() != requirement);
        let decision = inputs.evaluate();
        assert_eq!(decision.verdict(), ReleaseVerdict::NotReadyForProduction);
        assert!(decision.diagnostics().contains(&Diagnostic::MissingEvidence(requirement)));
        assert!(
            decision
                .criteria()
                .iter()
                .any(|value| value.criterion() == requirement.criterion() && !value.is_satisfied())
        );
    }
}

#[test]
fn decision_is_bound_to_the_exact_candidate_and_time() {
    let inputs = ready_inputs();
    let expected = inputs.candidate;
    let decision = inputs.evaluate();
    assert_eq!(decision.candidate(), expected);
    assert_eq!(decision.evaluated_at(), support::EVALUATED_AT);
}

#[test]
fn every_final_qualification_check_is_required_for_admission() {
    let decision = ready_inputs().evaluate();
    let complete = [ReleaseQualificationCheck::Satisfied; RELEASE_QUALIFICATION_CHECK_COUNT];
    assert!(ReleaseQualificationAdmission::evaluate(&decision, complete).is_ready());

    for index in 0..RELEASE_QUALIFICATION_CHECK_COUNT {
        let mut missing = complete;
        missing[index] = ReleaseQualificationCheck::NotSatisfied;
        assert!(!ReleaseQualificationAdmission::evaluate(&decision, missing).is_ready());
    }
}
