//! Canonical diagnostic emission from completed assessments.

use crate::{
    Diagnostic, EvidenceAssessment, FindingAssessment, QualificationAssessment, ReviewAssessment,
};
use vstd::prelude::*;

verus! {

pub(super) fn push_evidence(
    assessments: &[EvidenceAssessment; 44],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut index = 0;
    while index < assessments.len()
        invariant 0 <= index <= assessments.len(),
        decreases assessments.len() - index,
    {
        let value = assessments[index];
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
        index += 1;
    }
}

pub(super) fn push_qualifications(
    assessments: &[QualificationAssessment; 4],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut index = 0;
    while index < assessments.len()
        invariant 0 <= index <= assessments.len(),
        decreases assessments.len() - index,
    {
        let value = assessments[index];
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
        index += 1;
    }
}

pub(super) fn push_reviews(value: ReviewAssessment, diagnostics: &mut Vec<Diagnostic>) {
    if value.approved_count() < super::MIN_INDEPENDENT_REVIEWERS {
        diagnostics.push(Diagnostic::ReviewerQuorum {
            required: super::MIN_INDEPENDENT_REVIEWERS,
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
}

pub(super) fn push_findings(value: FindingAssessment, diagnostics: &mut Vec<Diagnostic>) {
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
}

} // verus!
