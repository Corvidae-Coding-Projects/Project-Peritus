//! Executable emission of the exact canonical diagnostic sequence.

use super::super::MIN_INDEPENDENT_REVIEWERS;
use crate::{
    Diagnostic, EvidenceAssessment, FindingAssessment, QualificationAssessment, ReviewAssessment,
};
use vstd::prelude::*;

verus! {

fn push_evidence_assessment(value: EvidenceAssessment, diagnostics: &mut Vec<Diagnostic>)
    ensures
        final(diagnostics)@ == old(diagnostics)@
            + super::evidence_assessment_diagnostics(value),
{
    if value.mismatched_count() > 0 {
        diagnostics.push(Diagnostic::MismatchedEvidence(
            value.requirement(),
            value.mismatched_count(),
        ));
    }
    if value.stale_count() > 0 {
        diagnostics.push(Diagnostic::StaleEvidence(
            value.requirement(),
            value.stale_count(),
        ));
    }
    if value.wrong_source_count() > 0 {
        diagnostics.push(Diagnostic::WrongEvidenceSource(
            value.requirement(),
            value.wrong_source_count(),
        ));
    }
    if value.unreviewed_count() > 0 {
        diagnostics.push(Diagnostic::UnreviewedEvidence(
            value.requirement(),
            value.unreviewed_count(),
        ));
    }
    if value.unsigned_count() > 0 {
        diagnostics.push(Diagnostic::UnsignedEvidence(
            value.requirement(),
            value.unsigned_count(),
        ));
    }
    if value.is_conflicting() {
        diagnostics.push(Diagnostic::ConflictingEvidence(value.requirement()));
    }
    if value.contributing_count() == 0 {
        diagnostics.push(Diagnostic::MissingEvidence(value.requirement()));
    }
    proof { reveal(super::evidence_assessment_diagnostics); };
}

/// Appends exact evidence diagnostics in stable requirement order.
pub(in crate::evaluator) fn push_evidence(
    assessments: &[EvidenceAssessment; 44],
    diagnostics: &mut Vec<Diagnostic>,
)
    ensures
        final(diagnostics)@.len() >= old(diagnostics)@.len(),
        (final(diagnostics)@.len() == old(diagnostics)@.len()) == super::evidence_clear(assessments),
        final(diagnostics)@ == old(diagnostics)@
            + super::evidence_diagnostics_through(assessments@, assessments@.len() as nat),
{
    let mut index = 0;
    while index < assessments.len()
        invariant
            0 <= index <= assessments.len(),
            diagnostics@ == old(diagnostics)@
                + super::evidence_diagnostics_through(assessments@, index as nat),
        decreases assessments.len() - index,
    {
        let value = assessments[index];
        push_evidence_assessment(value, diagnostics);
        proof {
            reveal(super::evidence_diagnostics_through);
        }
        index += 1;
    }
    proof {
        reveal(super::evidence_clear);
        super::evidence_diagnostics_empty_iff_clear(
            assessments@,
            assessments@.len() as nat,
        );
    };
}

fn push_qualification_assessment(
    value: QualificationAssessment,
    diagnostics: &mut Vec<Diagnostic>,
)
    ensures
        final(diagnostics)@ == old(diagnostics)@
            + super::qualification_assessment_diagnostics(value),
{
    if value.mismatched_count() > 0 {
        diagnostics.push(Diagnostic::MismatchedQualification(
            value.slice(),
            value.mismatched_count(),
        ));
    }
    if value.stale_count() > 0 {
        diagnostics.push(Diagnostic::StaleQualification(
            value.slice(),
            value.stale_count(),
        ));
    }
    if value.unreviewed_count() > 0 {
        diagnostics.push(Diagnostic::UnreviewedQualification(
            value.slice(),
            value.unreviewed_count(),
        ));
    }
    if value.not_ready_count() > 0 {
        diagnostics.push(Diagnostic::QualificationNotReady(
            value.slice(),
            value.not_ready_count(),
        ));
    }
    if value.is_conflicting() {
        diagnostics.push(Diagnostic::ConflictingQualification(value.slice()));
    }
    if value.ready_count() == 0 {
        diagnostics.push(Diagnostic::MissingQualification(value.slice()));
    }
    proof { reveal(super::qualification_assessment_diagnostics); };
}

/// Appends exact qualification diagnostics in H0-H3 order.
pub(in crate::evaluator) fn push_qualifications(
    assessments: &[QualificationAssessment; 4],
    diagnostics: &mut Vec<Diagnostic>,
)
    ensures
        final(diagnostics)@.len() >= old(diagnostics)@.len(),
        (final(diagnostics)@.len() == old(diagnostics)@.len())
            == super::qualifications_clear(assessments),
        final(diagnostics)@ == old(diagnostics)@
            + super::qualification_diagnostics_through(assessments@, assessments@.len() as nat),
{
    let mut index = 0;
    while index < assessments.len()
        invariant
            0 <= index <= assessments.len(),
            diagnostics@ == old(diagnostics)@
                + super::qualification_diagnostics_through(assessments@, index as nat),
        decreases assessments.len() - index,
    {
        let value = assessments[index];
        push_qualification_assessment(value, diagnostics);
        proof {
            reveal(super::qualification_diagnostics_through);
        }
        index += 1;
    }
    proof {
        reveal(super::qualifications_clear);
        super::qualification_diagnostics_empty_iff_clear(
            assessments@,
            assessments@.len() as nat,
        );
    };
}

/// Appends exact aggregate review diagnostics in canonical field order.
pub(in crate::evaluator) fn push_reviews(value: ReviewAssessment, diagnostics: &mut Vec<Diagnostic>)
    ensures
        final(diagnostics)@.len() >= old(diagnostics)@.len(),
        (final(diagnostics)@.len() == old(diagnostics)@.len())
            == value.spec_diagnostics_clear(),
        final(diagnostics)@ == old(diagnostics)@ + super::review_diagnostics(value),
{
    proof {
        reveal(ReviewAssessment::spec_diagnostics_clear);
    }
    if value.approved_count() < MIN_INDEPENDENT_REVIEWERS {
        diagnostics.push(Diagnostic::ReviewerQuorum {
            required: MIN_INDEPENDENT_REVIEWERS,
            observed: value.approved_count(),
        });
    }
    if value.mismatched_count() > 0 {
        diagnostics.push(Diagnostic::MismatchedReviews(value.mismatched_count()));
    }
    if value.stale_count() > 0 {
        diagnostics.push(Diagnostic::StaleReviews(value.stale_count()));
    }
    if value.changes_required_count() > 0 {
        diagnostics.push(Diagnostic::ChangesRequired(value.changes_required_count()));
    }
    if value.self_review_count() > 0 {
        diagnostics.push(Diagnostic::SelfReview(value.self_review_count()));
    }
    if value.non_independent_count() > 0 {
        diagnostics.push(Diagnostic::NonIndependentReview(value.non_independent_count()));
    }
    if value.has_duplicate_reviewer() {
        diagnostics.push(Diagnostic::DuplicateReviewer);
    }
    if value.has_shared_context() {
        diagnostics.push(Diagnostic::SharedReviewContext);
    }
    if value.has_conflicting_review() {
        diagnostics.push(Diagnostic::ConflictingReview);
    }
    proof { reveal(super::review_diagnostics); };
}

/// Appends exact aggregate finding diagnostics in canonical field order.
pub(in crate::evaluator) fn push_findings(value: FindingAssessment, diagnostics: &mut Vec<Diagnostic>)
    ensures
        final(diagnostics)@.len() >= old(diagnostics)@.len(),
        (final(diagnostics)@.len() == old(diagnostics)@.len())
            == value.spec_diagnostics_clear(),
        final(diagnostics)@ == old(diagnostics)@ + super::finding_diagnostics(value),
{
    proof {
        reveal(FindingAssessment::spec_diagnostics_clear);
    }
    if value.mismatched_count() > 0 {
        diagnostics.push(Diagnostic::MismatchedFindingState(value.mismatched_count()));
    }
    if value.stale_count() > 0 {
        diagnostics.push(Diagnostic::StaleFindingState(value.stale_count()));
    }
    if value.open_count() > 0 {
        diagnostics.push(Diagnostic::OpenFindings(value.open_count()));
    }
    if value.release_blocking_count() > 0 {
        diagnostics.push(Diagnostic::ReleaseBlockingFindings(
            value.release_blocking_count(),
        ));
    }
    if value.ignored_count() > 0 {
        diagnostics.push(Diagnostic::IgnoredFindings(value.ignored_count()));
    }
    if value.quarantined_count() > 0 {
        diagnostics.push(Diagnostic::QuarantinedFindings(value.quarantined_count()));
    }
    if value.invalid_waiver_count() > 0 {
        diagnostics.push(Diagnostic::InvalidWaivers(value.invalid_waiver_count()));
    }
    if value.has_conflicting_finding() {
        diagnostics.push(Diagnostic::ConflictingFinding);
    }
    proof { reveal(super::finding_diagnostics); };
}

} // verus!
