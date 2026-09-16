//! Actual-input specification for independent review and finding closure.

#[cfg(verus_only)]
use crate::{
    FindingObservation, IndependentSecurityReview, IntegratedCandidate, ReviewScope,
    SecurityEvidence,
};
use vstd::prelude::*;

verus! {

pub open spec fn scope_present(scopes: Seq<ReviewScope>, target: ReviewScope) -> bool {
    exists |index: int| 0 <= index < scopes.len() && #[trigger] scopes[index] == target
}

pub open spec fn scopes_complete_through(
    review: &IndependentSecurityReview,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] scope_present(review.spec_scopes(), ReviewScope::ALL[index])
}

pub open spec fn scopes_complete(review: &IndependentSecurityReview) -> bool {
    scopes_complete_through(review, ReviewScope::ALL.len() as int)
}

pub open spec fn review_complete_for(
    review: &IndependentSecurityReview,
    candidate: IntegratedCandidate,
) -> bool {
    crate::binding::candidate_fresh(review.spec_candidate(), candidate)
        && review.spec_completion().spec_is_completed()
        && review.spec_independent_from_producer()
        && crate::binding::digest_is_present(review.spec_report_digest())
        && scopes_complete(review)
}

pub open spec fn review_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    match evidence.spec_review() {
        Some(review) => review_complete_for(&review, candidate),
        None => false,
    }
}

pub open spec fn finding_clear(
    finding: FindingObservation,
    candidate: IntegratedCandidate,
) -> bool {
    crate::binding::candidate_fresh(finding.spec_candidate(), candidate)
        && (!finding.spec_severity().spec_is_release_blocking()
            || finding.spec_lifecycle().spec_has_resolution_evidence())
}

pub open spec fn findings_clear_through(
    review: &IndependentSecurityReview,
    candidate: IntegratedCandidate,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] finding_clear(review.spec_findings()[index], candidate)
}

pub open spec fn blockers_clear_for(
    review: &IndependentSecurityReview,
    candidate: IntegratedCandidate,
) -> bool {
    crate::binding::candidate_fresh(review.spec_candidate(), candidate)
        && findings_clear_through(review, candidate, review.spec_findings().len() as int)
}

pub open spec fn blockers_clear(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    match evidence.spec_review() {
        Some(review) => blockers_clear_for(&review, candidate),
        None => false,
    }
}

pub proof fn scopes_complete_step(review: &IndependentSecurityReview, end: int)
    requires 0 <= end < ReviewScope::ALL.len(),
    ensures scopes_complete_through(review, end + 1) == (
        scopes_complete_through(review, end)
            && scope_present(review.spec_scopes(), ReviewScope::ALL[end])
    ),
{
    let current = scope_present(review.spec_scopes(), ReviewScope::ALL[end]);
    if scopes_complete_through(review, end + 1) {
        assert(scopes_complete_through(review, end));
        assert(current);
    } else if scopes_complete_through(review, end) && current {
        assert(scopes_complete_through(review, end + 1)) by {
            assert forall |index: int| 0 <= index < end + 1 implies
                #[trigger] scope_present(review.spec_scopes(), ReviewScope::ALL[index]) by {
                if index < end {
                    assert(scope_present(review.spec_scopes(), ReviewScope::ALL[index]));
                } else {
                    assert(index == end);
                }
            }
        };
    }
}

pub proof fn findings_clear_step(
    review: &IndependentSecurityReview,
    candidate: IntegratedCandidate,
    end: int,
)
    requires 0 <= end < review.spec_findings().len(),
    ensures findings_clear_through(review, candidate, end + 1) == (
        findings_clear_through(review, candidate, end)
            && finding_clear(review.spec_findings()[end], candidate)
    ),
{
    let current = finding_clear(review.spec_findings()[end], candidate);
    if findings_clear_through(review, candidate, end + 1) {
        assert(findings_clear_through(review, candidate, end));
        assert(current);
    } else if findings_clear_through(review, candidate, end) && current {
        assert(findings_clear_through(review, candidate, end + 1)) by {
            assert forall |index: int| 0 <= index < end + 1 implies
                #[trigger] finding_clear(review.spec_findings()[index], candidate) by {
                if index < end {
                    assert(finding_clear(review.spec_findings()[index], candidate));
                } else {
                    assert(index == end);
                }
            }
        };
    }
}

} // verus!
