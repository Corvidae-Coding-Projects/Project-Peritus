//! Independent review completion and release-blocking finding closure.

use crate::{
    IntegratedCandidate, ReviewCompletion, ReviewScope, SecurityEvidence, UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

fn has_scope(scopes: &[ReviewScope], expected: ReviewScope) -> (found: bool) {
    let mut index = 0;
    while index < scopes.len()
        invariant 0 <= index <= scopes@.len(),
        decreases scopes@.len() - index,
    {
        if scopes[index] == expected {
            return true;
        }
        index += 1;
    }
    false
}

pub(super) fn evaluate(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> (bool, bool) {
    let Some(review) = evidence.review() else {
        unmet.push(UnmetSecurityCondition::MissingExternalReview);
        return (false, false);
    };
    let current = crate::binding::candidate_matches(review.candidate(), candidate);
    let mut review_complete = current;
    if review.completion() != ReviewCompletion::Completed {
        review_complete = false;
        unmet.push(UnmetSecurityCondition::ExternalReviewIncomplete);
    }
    if !review.independent_from_producer() {
        review_complete = false;
        unmet.push(UnmetSecurityCondition::ExternalReviewNotIndependent);
    }
    if !crate::binding::digest_present(review.report_digest()) {
        review_complete = false;
        unmet.push(UnmetSecurityCondition::EmptyExternalReviewDigest);
    }
    let mut scope_index = 0;
    while scope_index < ReviewScope::ALL.len()
        invariant 0 <= scope_index <= ReviewScope::ALL.len(),
        decreases ReviewScope::ALL.len() - scope_index,
    {
        let scope = ReviewScope::ALL[scope_index];
        if !has_scope(review.scopes(), scope) {
            review_complete = false;
            unmet.push(UnmetSecurityCondition::MissingExternalReviewScope(scope));
        }
        scope_index += 1;
    }

    let mut blockers_clear = current;
    let mut index = 0;
    while index < review.findings().len()
        invariant 0 <= index <= review.spec_findings().len(),
        decreases review.spec_findings().len() - index,
    {
        let finding = &review.findings()[index];
        if !crate::binding::candidate_matches(finding.candidate(), candidate) {
            blockers_clear = false;
        }
        if finding.severity().is_release_blocking()
            && !finding.lifecycle().has_resolution_evidence()
        {
            blockers_clear = false;
            unmet.push(UnmetSecurityCondition::UnresolvedReleaseBlocker {
                finding_id: finding.finding_id(),
                severity: finding.severity(),
            });
        }
        index += 1;
    }
    (review_complete, blockers_clear)
}

} // verus!
