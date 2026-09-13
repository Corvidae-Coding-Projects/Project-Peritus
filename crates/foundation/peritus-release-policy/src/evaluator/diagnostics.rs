//! Canonical diagnostic emission from completed assessments.

#[cfg(verus_only)]
use crate::{
    Diagnostic, EvidenceAssessment, FindingAssessment, QualificationAssessment, ReviewAssessment,
};
use vstd::prelude::*;

pub(super) mod emission;

verus! {

pub open spec fn evidence_assessment_diagnostics(
    value: EvidenceAssessment,
) -> Seq<Diagnostic> {
    let result = Seq::empty();
    let result = if value.spec_mismatched_count() > 0 {
        result.push(Diagnostic::MismatchedEvidence(
            value.spec_requirement(), value.spec_mismatched_count()))
    } else { result };
    let result = if value.spec_stale_count() > 0 {
        result.push(Diagnostic::StaleEvidence(
            value.spec_requirement(), value.spec_stale_count()))
    } else { result };
    let result = if value.spec_wrong_source_count() > 0 {
        result.push(Diagnostic::WrongEvidenceSource(
            value.spec_requirement(), value.spec_wrong_source_count()))
    } else { result };
    let result = if value.spec_unreviewed_count() > 0 {
        result.push(Diagnostic::UnreviewedEvidence(
            value.spec_requirement(), value.spec_unreviewed_count()))
    } else { result };
    let result = if value.spec_unsigned_count() > 0 {
        result.push(Diagnostic::UnsignedEvidence(
            value.spec_requirement(), value.spec_unsigned_count()))
    } else { result };
    let result = if value.spec_is_conflicting() {
        result.push(Diagnostic::ConflictingEvidence(value.spec_requirement()))
    } else { result };
    if value.spec_contributing_count() == 0 {
        result.push(Diagnostic::MissingEvidence(value.spec_requirement()))
    } else { result }
}

pub open spec fn evidence_diagnostics_through(
    values: Seq<EvidenceAssessment>,
    end: nat,
) -> Seq<Diagnostic>
    decreases end,
{
    if end == 0 {
        Seq::empty()
    } else {
        evidence_diagnostics_through(values, (end - 1) as nat)
            + evidence_assessment_diagnostics(values[(end - 1) as int])
    }
}

pub open spec fn qualification_assessment_diagnostics(
    value: QualificationAssessment,
) -> Seq<Diagnostic> {
    let result = Seq::empty();
    let result = if value.spec_mismatched_count() > 0 {
        result.push(Diagnostic::MismatchedQualification(
            value.spec_slice(), value.spec_mismatched_count()))
    } else { result };
    let result = if value.spec_stale_count() > 0 {
        result.push(Diagnostic::StaleQualification(
            value.spec_slice(), value.spec_stale_count()))
    } else { result };
    let result = if value.spec_unreviewed_count() > 0 {
        result.push(Diagnostic::UnreviewedQualification(
            value.spec_slice(), value.spec_unreviewed_count()))
    } else { result };
    let result = if value.spec_not_ready_count() > 0 {
        result.push(Diagnostic::QualificationNotReady(
            value.spec_slice(), value.spec_not_ready_count()))
    } else { result };
    let result = if value.spec_is_conflicting() {
        result.push(Diagnostic::ConflictingQualification(value.spec_slice()))
    } else { result };
    if value.spec_ready_count() == 0 {
        result.push(Diagnostic::MissingQualification(value.spec_slice()))
    } else { result }
}

pub open spec fn qualification_diagnostics_through(
    values: Seq<QualificationAssessment>,
    end: nat,
) -> Seq<Diagnostic>
    decreases end,
{
    if end == 0 {
        Seq::empty()
    } else {
        qualification_diagnostics_through(values, (end - 1) as nat)
            + qualification_assessment_diagnostics(values[(end - 1) as int])
    }
}

pub open spec fn review_diagnostics(value: ReviewAssessment) -> Seq<Diagnostic> {
    let result = Seq::empty();
    let result = if value.spec_approved_count() < super::MIN_INDEPENDENT_REVIEWERS {
        result.push(Diagnostic::ReviewerQuorum {
            required: super::MIN_INDEPENDENT_REVIEWERS,
            observed: value.spec_approved_count(),
        })
    } else { result };
    let result = if value.spec_mismatched_count() > 0 {
        result.push(Diagnostic::MismatchedReviews(value.spec_mismatched_count()))
    } else { result };
    let result = if value.spec_stale_count() > 0 {
        result.push(Diagnostic::StaleReviews(value.spec_stale_count()))
    } else { result };
    let result = if value.spec_changes_required_count() > 0 {
        result.push(Diagnostic::ChangesRequired(value.spec_changes_required_count()))
    } else { result };
    let result = if value.spec_self_review_count() > 0 {
        result.push(Diagnostic::SelfReview(value.spec_self_review_count()))
    } else { result };
    let result = if value.spec_non_independent_count() > 0 {
        result.push(Diagnostic::NonIndependentReview(value.spec_non_independent_count()))
    } else { result };
    let result = if value.spec_duplicate_reviewer() {
        result.push(Diagnostic::DuplicateReviewer)
    } else { result };
    let result = if value.spec_shared_context() {
        result.push(Diagnostic::SharedReviewContext)
    } else { result };
    if value.spec_conflicting_review() {
        result.push(Diagnostic::ConflictingReview)
    } else { result }
}

pub open spec fn finding_diagnostics(value: FindingAssessment) -> Seq<Diagnostic> {
    let result = Seq::empty();
    let result = if value.spec_mismatched_count() > 0 {
        result.push(Diagnostic::MismatchedFindingState(value.spec_mismatched_count()))
    } else { result };
    let result = if value.spec_stale_count() > 0 {
        result.push(Diagnostic::StaleFindingState(value.spec_stale_count()))
    } else { result };
    let result = if value.spec_open_count() > 0 {
        result.push(Diagnostic::OpenFindings(value.spec_open_count()))
    } else { result };
    let result = if value.spec_release_blocking_count() > 0 {
        result.push(Diagnostic::ReleaseBlockingFindings(
            value.spec_release_blocking_count()))
    } else { result };
    let result = if value.spec_ignored_count() > 0 {
        result.push(Diagnostic::IgnoredFindings(value.spec_ignored_count()))
    } else { result };
    let result = if value.spec_quarantined_count() > 0 {
        result.push(Diagnostic::QuarantinedFindings(value.spec_quarantined_count()))
    } else { result };
    let result = if value.spec_invalid_waiver_count() > 0 {
        result.push(Diagnostic::InvalidWaivers(value.spec_invalid_waiver_count()))
    } else { result };
    if value.spec_conflicting_finding() {
        result.push(Diagnostic::ConflictingFinding)
    } else { result }
}

/// Exact canonical diagnostic variants, payloads, and ordering for completed assessments.
pub open spec fn canonical_diagnostics(
    evidence: Seq<EvidenceAssessment>,
    qualifications: Seq<QualificationAssessment>,
    reviews: ReviewAssessment,
    findings: FindingAssessment,
) -> Seq<Diagnostic> {
    evidence_diagnostics_through(evidence, evidence.len() as nat)
        + qualification_diagnostics_through(qualifications, qualifications.len() as nat)
        + review_diagnostics(reviews)
        + finding_diagnostics(findings)
}

pub(in crate::evaluator) open spec fn evidence_clear_through(
    values: Seq<EvidenceAssessment>,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        evidence_clear_through(values, (end - 1) as nat)
            && values[(end - 1) as int].spec_diagnostics_clear()
    }
}

pub(in crate::evaluator) open spec fn evidence_clear(assessments: &[EvidenceAssessment; 44]) -> bool {
    evidence_clear_through(assessments@, 44)
}

pub(in crate::evaluator) proof fn evidence_diagnostics_empty_iff_clear(
    values: Seq<EvidenceAssessment>,
    end: nat,
)
    requires end <= values.len(),
    ensures
        (evidence_diagnostics_through(values, end).len() == 0)
            == evidence_clear_through(values, end),
    decreases end,
{
    if end > 0 {
        evidence_diagnostics_empty_iff_clear(values, (end - 1) as nat);
        reveal(evidence_diagnostics_through);
        reveal(evidence_assessment_diagnostics);
        reveal(evidence_clear_through);
        reveal(EvidenceAssessment::spec_diagnostics_clear);
    }
}

pub(in crate::evaluator) open spec fn qualifications_clear_through(
    values: Seq<QualificationAssessment>,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        qualifications_clear_through(values, (end - 1) as nat)
            && values[(end - 1) as int].spec_diagnostics_clear()
    }
}

pub(in crate::evaluator) open spec fn qualifications_clear(assessments: &[QualificationAssessment; 4]) -> bool {
    qualifications_clear_through(assessments@, 4)
}

pub(in crate::evaluator) proof fn qualification_diagnostics_empty_iff_clear(
    values: Seq<QualificationAssessment>,
    end: nat,
)
    requires end <= values.len(),
    ensures
        (qualification_diagnostics_through(values, end).len() == 0)
            == qualifications_clear_through(values, end),
    decreases end,
{
    if end > 0 {
        qualification_diagnostics_empty_iff_clear(values, (end - 1) as nat);
        reveal(qualification_diagnostics_through);
        reveal(qualification_assessment_diagnostics);
        reveal(qualifications_clear_through);
        reveal(QualificationAssessment::spec_diagnostics_clear);
    }
}

} // verus!
