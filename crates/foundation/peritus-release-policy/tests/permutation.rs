//! Release decision permutation-invariance contracts.

mod support;

use peritus_release_policy::ReleaseEvidence;
use support::{EVALUATED_AT, ready_inputs};

#[test]
fn evidence_permutations_preserve_verdict_digest_and_diagnostics() {
    let canonical = ready_inputs();
    let candidate = canonical.candidate;
    let canonical_decision = canonical.evaluate();

    let mut permuted = ready_inputs();
    permuted.observations.reverse();
    permuted.qualifications.reverse();
    permuted.reviews.reverse();
    let evidence = ReleaseEvidence::new(
        permuted.observations,
        permuted.qualifications,
        permuted.reviews,
        permuted.findings,
        permuted.waivers,
    )
    .expect("permuted evidence remains bounded");
    let permuted_decision =
        peritus_release_policy::evaluate_release(candidate, EVALUATED_AT, &evidence);

    assert_eq!(canonical_decision.verdict(), permuted_decision.verdict());
    assert_eq!(canonical_decision.digest(), permuted_decision.digest());
    assert_eq!(canonical_decision.criteria(), permuted_decision.criteria());
    assert_eq!(canonical_decision.evidence(), permuted_decision.evidence());
    assert_eq!(canonical_decision.qualifications(), permuted_decision.qualifications());
    assert_eq!(canonical_decision.reviews(), permuted_decision.reviews());
    assert_eq!(canonical_decision.findings(), permuted_decision.findings());
    assert_eq!(canonical_decision.diagnostics(), permuted_decision.diagnostics());
}
