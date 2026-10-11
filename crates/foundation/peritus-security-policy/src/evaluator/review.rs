//! Independent review completion and release-blocking finding closure.

mod model;

use crate::{IntegratedCandidate, ReviewScope, SecurityEvidence, UnmetSecurityCondition};

const fn scopes_equal(left: ReviewScope, right: ReviewScope) -> bool {
    matches!(
        (left, right),
        (ReviewScope::SandboxEscape, ReviewScope::SandboxEscape)
            | (ReviewScope::AuthorityIsolation, ReviewScope::AuthorityIsolation)
            | (ReviewScope::EvolutionAndPromotion, ReviewScope::EvolutionAndPromotion)
            | (ReviewScope::SupplyChain, ReviewScope::SupplyChain)
            | (
                ReviewScope::UnsafeAndTrustedComputingBase,
                ReviewScope::UnsafeAndTrustedComputingBase
            )
    )
}
fn has_scope(scopes: &[ReviewScope], expected: ReviewScope) -> bool {
    let mut index = 0;
    while index < scopes.len() {
        if scopes_equal(scopes[index], expected) {
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
    let completion = review.completion().is_completed();
    if !completion {
        review_complete = false;
        unmet.push(UnmetSecurityCondition::ExternalReviewIncomplete);
    }
    let independent = review.independent_from_producer();
    if !independent {
        review_complete = false;
        unmet.push(UnmetSecurityCondition::ExternalReviewNotIndependent);
    }
    let report_present = crate::binding::digest_present(review.report_digest());
    if !report_present {
        review_complete = false;
        unmet.push(UnmetSecurityCondition::EmptyExternalReviewDigest);
    }
    let mut scope_index = 0;
    while scope_index < ReviewScope::ALL.len() {
        let scope = ReviewScope::ALL[scope_index];
        if !has_scope(review.scopes(), scope) {
            review_complete = false;
            unmet.push(UnmetSecurityCondition::MissingExternalReviewScope(scope));
        }
        scope_index += 1;
    }
    let mut blockers_clear = current;
    let mut index = 0;
    while index < review.findings().len() {
        let finding = &review.findings()[index];
        let finding_current = crate::binding::candidate_matches(finding.candidate(), candidate);
        if !finding_current {
            blockers_clear = false;
        }
        let blocking = finding.severity().is_release_blocking();
        let resolved = finding.lifecycle().has_resolution_evidence();
        if blocking && !resolved {
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
