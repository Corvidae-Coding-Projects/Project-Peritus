//! Total deterministic H4 release evaluation.

use crate::{Diagnostic, ReleaseCandidate, ReleaseDecision, ReleaseEvidence};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

mod admission;
mod artifacts;
mod diagnostics;
mod findings;
mod qualifications;
mod reviews;

#[cfg(verus_only)]
pub use self::admission::{evidence_requirement_admitted, required_evidence_admitted};

/// Minimum distinct independent approvals required by H4.
pub const MIN_INDEPENDENT_REVIEWERS: u16 = 2;

pub open spec fn saturated_increment(value: u16) -> u16 {
    if value == 65535u16 { value } else { (value + 1) as u16 }
}

pub open spec fn xor_digest_bytes(bytes: Seq<u8>, digest: Sha256Digest) -> Seq<u8> {
    Seq::new(32, |index: int| bytes[index] ^ digest.spec_bytes()[index])
}

/// Declarative readiness over every supplied release-policy input.
pub open spec fn release_inputs_ready(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    artifacts::all_requirements_satisfied(evidence, candidate, evaluated_at)
        && artifacts::all_criteria_satisfied(evidence, candidate, evaluated_at)
        && qualifications::all_qualifications_satisfied(evidence, candidate, evaluated_at)
        && reviews::review_satisfied(evidence, candidate, evaluated_at)
        && findings::findings_satisfied(evidence, candidate, evaluated_at)
}

/// Formal correspondence established between evaluator inputs and its stored decision.
pub open spec fn ready_evaluation_contract(
    decision: &ReleaseDecision,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> bool {
    decision.spec_candidate() == candidate
        && decision.spec_evaluated_at() == evaluated_at
        && artifacts::assessments_match_inputs(
            decision.spec_evidence(), evidence, candidate, evaluated_at)
        && artifacts::criteria_assessments_match_inputs(
            decision.spec_criteria(), decision.spec_evidence())
        && qualifications::assessments_match_inputs(
            decision.spec_qualifications(), evidence, candidate, evaluated_at)
        && reviews::assessment_matches_reduction(
            decision.spec_reviews(), evidence, candidate, evaluated_at)
        && findings::assessment_matches_reduction(
            decision.spec_findings(), evidence, candidate, evaluated_at)
        && decision.spec_diagnostics() == diagnostics::canonical_diagnostics(
            decision.spec_evidence(),
            decision.spec_qualifications(),
            decision.spec_reviews(),
            decision.spec_findings(),
        )
        && decision.spec_digest().spec_bytes()@ == crate::decision::expected_decision_digest(
            candidate.spec_manifest_digest(),
            decision.spec_verdict(),
            decision.spec_evidence(),
            decision.spec_qualifications(),
            decision.spec_reviews(),
            decision.spec_findings(),
        )
        && decision.spec_required_artifacts_complete()
            == artifacts::all_requirements_satisfied(evidence, candidate, evaluated_at)
        && decision.spec_all_criteria_satisfied()
            == artifacts::all_criteria_satisfied(evidence, candidate, evaluated_at)
        && decision.spec_all_qualifications_ready()
            == qualifications::all_qualifications_satisfied(evidence, candidate, evaluated_at)
        && decision.spec_reviews_complete()
            == reviews::review_satisfied(evidence, candidate, evaluated_at)
        && decision.spec_blockers_absent()
            == findings::findings_satisfied(evidence, candidate, evaluated_at)
        && ((decision.spec_verdict() == crate::ReleaseVerdict::Ready)
            == release_inputs_ready(evidence, candidate, evaluated_at))
        && (decision.spec_is_ready()
            == release_inputs_ready(evidence, candidate, evaluated_at))
        && ((decision.spec_diagnostics().len() == 0)
            == release_inputs_ready(evidence, candidate, evaluated_at))
        && (decision.spec_is_ready() ==> {
            &&& required_evidence_admitted(evidence, candidate, evaluated_at)
            &&& decision.spec_all_criteria_satisfied()
            &&& decision.spec_required_artifacts_complete()
            &&& decision.spec_all_qualifications_ready()
            &&& decision.spec_reviews_complete()
            &&& decision.spec_blockers_absent()
            &&& decision.spec_diagnostics().len() == 0
        })
}

/// Evaluates all H4 production obligations for one exact release candidate.
///
/// The evaluator performs no I/O and grants no publication authority. Input order does not affect
/// output order: requirements, criteria, and H0-H3 inputs are always assessed in their closed
/// stable-ID order; review/finding diagnostics are aggregate facts rather than input positions.
#[must_use]
pub fn evaluate_release(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (decision: ReleaseDecision)
    ensures ready_evaluation_contract(&decision, candidate, evaluated_at, evidence)
{
    let evidence_assessments = artifacts::assess_all_evidence(
        &candidate,
        evaluated_at,
        evidence,
    );

    let criteria = artifacts::assess_all_criteria(&evidence_assessments);

    let qualifications = qualifications::assess_all(&candidate, evaluated_at, evidence);

    let reviews = reviews::assess(candidate, evaluated_at, evidence);
    let findings = findings::assess(candidate, evaluated_at, evidence);
    let mut diagnostics = Vec::<Diagnostic>::new();
    diagnostics::emission::push_evidence(&evidence_assessments, &mut diagnostics);
    diagnostics::emission::push_qualifications(&qualifications, &mut diagnostics);
    diagnostics::emission::push_reviews(reviews, &mut diagnostics);
    diagnostics::emission::push_findings(findings, &mut diagnostics);
    proof {
        artifacts::criteria_cover_all_requirements(evidence, candidate, evaluated_at);
        reveal(release_inputs_ready);
        assert((diagnostics@.len() == 0)
            == release_inputs_ready(evidence, candidate, evaluated_at));
    }

    let decision = ReleaseDecision::from_evaluation(
        candidate,
        evaluated_at,
        criteria,
        evidence_assessments,
        qualifications,
        reviews,
        findings,
        diagnostics,
    );
    proof {
        artifacts::criteria_cover_all_requirements(evidence, candidate, evaluated_at);
        reveal(release_inputs_ready);
        reveal(crate::decision::evaluation_components_ready);
        assert(decision.spec_candidate() == candidate);
        assert(decision.spec_evaluated_at() == evaluated_at);
        assert(decision.spec_evidence() == evidence_assessments@);
        assert(decision.spec_criteria() == criteria@);
        assert(decision.spec_qualifications() == qualifications@);
        assert(artifacts::assessments_match_inputs(
            decision.spec_evidence(), evidence, candidate, evaluated_at));
        assert(artifacts::criteria_assessments_match_inputs(
            decision.spec_criteria(), decision.spec_evidence()));
        assert(qualifications::assessments_match_inputs(
            decision.spec_qualifications(), evidence, candidate, evaluated_at));
        assert(reviews::assessment_matches_reduction(
            decision.spec_reviews(), evidence, candidate, evaluated_at));
        assert(findings::assessment_matches_reduction(
            decision.spec_findings(), evidence, candidate, evaluated_at));
        assert(decision.spec_diagnostics() == diagnostics::canonical_diagnostics(
            decision.spec_evidence(),
            decision.spec_qualifications(),
            decision.spec_reviews(),
            decision.spec_findings(),
        ));
        assert(decision.spec_digest().spec_bytes()@
            == crate::decision::expected_decision_digest(
                candidate.spec_manifest_digest(),
                decision.spec_verdict(),
                decision.spec_evidence(),
                decision.spec_qualifications(),
                decision.spec_reviews(),
                decision.spec_findings(),
            ));
        assert(decision.spec_required_artifacts_complete()
            == artifacts::all_requirements_satisfied(evidence, candidate, evaluated_at));
        assert(decision.spec_all_criteria_satisfied()
            == artifacts::all_criteria_satisfied(evidence, candidate, evaluated_at));
        assert(decision.spec_all_qualifications_ready()
            == qualifications::all_qualifications_satisfied(evidence, candidate, evaluated_at));
        assert(decision.spec_reviews_complete()
            == reviews::review_satisfied(evidence, candidate, evaluated_at));
        assert(decision.spec_blockers_absent()
            == findings::findings_satisfied(evidence, candidate, evaluated_at));
        assert((decision.spec_verdict() == crate::ReleaseVerdict::Ready)
            == release_inputs_ready(evidence, candidate, evaluated_at));
        assert(decision.spec_is_ready()
            == release_inputs_ready(evidence, candidate, evaluated_at));
        assert((decision.spec_diagnostics().len() == 0)
            == release_inputs_ready(evidence, candidate, evaluated_at));
        if decision.spec_is_ready() {
            decision.ready_implies_final_obligations();
            assert(required_evidence_admitted(evidence, candidate, evaluated_at));
        }
        reveal(ready_evaluation_contract);
    }
    decision
}

const fn increment(value: &mut u16)
    ensures *final(value) == saturated_increment(*old(value)),
{
    *value = (*value).saturating_add(1);
}

const fn xor_digest(output: &mut [u8; 32], digest: Sha256Digest)
    ensures final(output)@ == xor_digest_bytes(old(output)@, digest),
{
    let mut index = 0;
    while index < output.len()
        invariant
            0 <= index <= output.len(),
            output@.len() == 32,
            old(output)@.len() == 32,
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] output@[prior]
                    == old(output)@[prior] ^ digest.spec_bytes()[prior],
            forall |remaining: int| index <= remaining < 32 ==>
                #[trigger] output@[remaining] == old(output)@[remaining],
        decreases output.len() - index,
    {
        output[index] ^= digest.as_bytes()[index];
        index += 1;
    }
    assert(output@ =~= xor_digest_bytes(old(output)@, digest));
}

} // verus!
