//! Independent review completion and release-blocking finding closure.

mod model;

use crate::{
    IntegratedCandidate, ReviewScope, SecurityEvidence, UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

pub open spec fn independent_review_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    model::review_complete(evidence, candidate)
}

pub open spec fn release_blockers_clear(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    model::blockers_clear(evidence, candidate)
}

const fn scopes_equal(left: ReviewScope, right: ReviewScope) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (ReviewScope::SandboxEscape, ReviewScope::SandboxEscape)
        | (ReviewScope::AuthorityIsolation, ReviewScope::AuthorityIsolation)
        | (ReviewScope::EvolutionAndPromotion, ReviewScope::EvolutionAndPromotion)
        | (ReviewScope::SupplyChain, ReviewScope::SupplyChain)
        | (ReviewScope::UnsafeAndTrustedComputingBase, ReviewScope::UnsafeAndTrustedComputingBase)
    )
}

fn has_scope(scopes: &[ReviewScope], expected: ReviewScope) -> (found: bool)
    ensures found == model::scope_present(scopes@, expected),
{
    let mut index = 0;
    while index < scopes.len()
        invariant
            0 <= index <= scopes@.len(),
            forall |prior: int| 0 <= prior < index ==> #[trigger] scopes@[prior] != expected,
        decreases scopes@.len() - index,
    {
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
) -> (result: (bool, bool))
    ensures
        result.0 == independent_review_complete(evidence, candidate),
        result.1 == release_blockers_clear(evidence, candidate),
        result.0 && result.1 ==> final(unmet)@ == old(unmet)@,
{
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
    while scope_index < ReviewScope::ALL.len()
        invariant
            0 <= scope_index <= ReviewScope::ALL.len(),
            review_complete == (
                current
                    && completion
                    && independent
                    && report_present
                    && model::scopes_complete_through(review, scope_index as int)
            ),
            review_complete ==> unmet@ == old(unmet)@,
        decreases ReviewScope::ALL.len() - scope_index,
    {
        let scope = ReviewScope::ALL[scope_index];
        if !has_scope(review.scopes(), scope) {
            review_complete = false;
            unmet.push(UnmetSecurityCondition::MissingExternalReviewScope(scope));
        }
        proof {
            model::scopes_complete_step(review, scope_index as int);
        }
        scope_index += 1;
    }

    reveal(model::scopes_complete);
    reveal(model::review_complete_for);
    reveal(model::review_complete);
    reveal(independent_review_complete);

    let mut blockers_clear = current;
    let mut index = 0;
    while index < review.findings().len()
        invariant
            0 <= index <= review.spec_findings().len(),
            blockers_clear == (
                current && model::findings_clear_through(review, candidate, index as int)
            ),
            review_complete && blockers_clear ==> unmet@ == old(unmet)@,
        decreases review.spec_findings().len() - index,
    {
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
        proof {
            model::findings_clear_step(review, candidate, index as int);
        }
        index += 1;
    }
    reveal(model::blockers_clear_for);
    reveal(model::blockers_clear);
    reveal(release_blockers_clear);
    (review_complete, blockers_clear)
}

} // verus!
