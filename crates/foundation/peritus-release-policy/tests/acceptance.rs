//! Complete exact evidence produces the sole H4 Ready path.

mod support;

use peritus_release_policy::{Diagnostic, REQUIRED_EVIDENCE, ReleaseVerdict};
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
