//! Mathematical evidence freshness and permutation predicates.

#[cfg(verus_only)]
use crate::{EvidenceObservation, EvidenceRequirement, ReleaseCandidate};
use vstd::prelude::*;

verus! {

/// Whether a sequence contains contributing current evidence for one requirement.
pub open spec fn has_contributing_evidence(
    observations: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    exists |index: int| 0 <= index < observations.len()
        && #[trigger] observations[index].spec_contributes_to(
            requirement,
            candidate,
            evaluated_at,
        )
}

/// Exact permutation relation used by H4's order-independence claim.
pub open spec fn evidence_permutation(
    left: Seq<EvidenceObservation>,
    right: Seq<EvidenceObservation>,
) -> bool {
    left.to_multiset() == right.to_multiset()
}

/// Proves that permuting observations cannot change whether current evidence exists.
pub proof fn permutation_preserves_contributing_evidence(
    left: Seq<EvidenceObservation>,
    right: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    requires evidence_permutation(left, right),
    ensures has_contributing_evidence(left, requirement, candidate, evaluated_at)
        == has_contributing_evidence(right, requirement, candidate, evaluated_at),
{
    reveal(evidence_permutation);
    reveal(has_contributing_evidence);
    if has_contributing_evidence(left, requirement, candidate, evaluated_at) {
        contributing_evidence_permutation_direction(
            left,
            right,
            requirement,
            candidate,
            evaluated_at,
        );
    }
    if has_contributing_evidence(right, requirement, candidate, evaluated_at) {
        contributing_evidence_permutation_direction(
            right,
            left,
            requirement,
            candidate,
            evaluated_at,
        );
    }
}

proof fn contributing_evidence_permutation_direction(
    left: Seq<EvidenceObservation>,
    right: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    requires
        left.to_multiset() == right.to_multiset(),
        has_contributing_evidence(left, requirement, candidate, evaluated_at),
    ensures has_contributing_evidence(right, requirement, candidate, evaluated_at),
{
    reveal(has_contributing_evidence);
    let index = choose |index: int| 0 <= index < left.len()
        && left[index].spec_contributes_to(requirement, candidate, evaluated_at);
    let observation = left[index];
    assert(left.contains(observation));
    vstd::seq_lib::to_multiset_contains(left, observation);
    assert(left.to_multiset().count(observation) > 0);
    assert(right.to_multiset().count(observation) > 0);
    vstd::seq_lib::to_multiset_contains(right, observation);
    assert(right.contains(observation));
    let matching = choose |matching: int| 0 <= matching < right.len()
        && right[matching] == observation;
    assert(right[matching].spec_contributes_to(requirement, candidate, evaluated_at));
}

} // verus!
