//! Total deterministic H4 release evaluation.

use crate::{Diagnostic, ReleaseCandidate, ReleaseDecision, ReleaseEvidence};
use peritus_types::Sha256Digest;

mod admission;
mod artifacts;
mod diagnostics;
mod findings;
mod qualifications;
mod reviews;
#[doc = " Minimum distinct independent approvals required by H4."]
pub const MIN_INDEPENDENT_REVIEWERS: u16 = 2;
#[doc = " Evaluates all H4 production obligations for one exact release candidate."]
#[doc = ""]
#[doc = " The evaluator performs no I/O and grants no publication authority. Input order does not affect"]
#[doc = " output order: requirements, criteria, and H0-H3 inputs are always assessed in their closed"]
#[doc = " stable-ID order; review/finding diagnostics are aggregate facts rather than input positions."]
#[must_use]
pub fn evaluate_release(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> ReleaseDecision {
    let evidence_assessments = artifacts::assess_all_evidence(&candidate, evaluated_at, evidence);
    let criteria = artifacts::assess_all_criteria(&evidence_assessments);
    let qualifications = qualifications::assess_all(&candidate, evaluated_at, evidence);
    let reviews = reviews::assess(candidate, evaluated_at, evidence);
    let findings = findings::assess(candidate, evaluated_at, evidence);
    let mut diagnostics = Vec::<Diagnostic>::new();
    diagnostics::emission::push_evidence(&evidence_assessments, &mut diagnostics);
    diagnostics::emission::push_qualifications(&qualifications, &mut diagnostics);
    diagnostics::emission::push_reviews(reviews, &mut diagnostics);
    diagnostics::emission::push_findings(findings, &mut diagnostics);
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
    decision
}
const fn increment(value: &mut u16) {
    *value = (*value).saturating_add(1);
}
const fn xor_digest(output: &mut [u8; 32], digest: Sha256Digest) {
    let mut index = 0;
    while index < output.len() {
        output[index] ^= digest.as_bytes()[index];
        index += 1;
    }
}
