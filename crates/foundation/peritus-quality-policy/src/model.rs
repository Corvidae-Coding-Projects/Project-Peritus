//! Mathematical acceptance and freshness predicates.

#[cfg(verus_only)]
use crate::{GateObservation, GateOutcome, ReviewObservation};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

mod artifacts;
pub mod approvals;
pub mod authority;
mod gates;
mod reviews;

pub use reviews::ReviewIndependenceDimension;
#[cfg(verus_only)]
pub use reviews::{
    categories_declared, category_present, current_category_covered, current_review_categories_declared,
    current_review_count_prefix, current_review_pair, current_reviews_attest_producer_independence,
    digests_match, duplicate_reviewer_actor, duplicate_reviewer_fact, required_review_categories_covered,
    required_reviews_complete, review_categories_match, review_independence_complete,
    reviewer_actors_match, reviewer_fact, saturated_review_count,
};

#[cfg(verus_only)]
pub use artifacts::{
    artifact_declared, current_artifact_present, current_artifacts_declared,
    evidence_requirement_matches, required_artifacts_complete, required_artifacts_present,
};
#[cfg(verus_only)]
pub use gates::{
    current_gate_matches, current_gates_declared, first_current_gate, first_current_gate_passed,
    gate_declared, gate_ids_match, required_gates_complete, required_gates_passed,
};

verus! {

/// INV-003 freshness: evidence is current exactly when its complete tuple equals the request.
pub open spec fn revision_fresh(
    observed: RevisionTuple,
    requested: RevisionTuple,
) -> bool {
    crate::revision::same_identifier(
        observed.spec_acceptance_spec_id().spec_bytes(),
        requested.spec_acceptance_spec_id().spec_bytes(),
    )
        && crate::revision::same_identifier(
            observed.spec_harness_id().spec_bytes(),
            requested.spec_harness_id().spec_bytes(),
        )
        && crate::revision::same_identifier(
            observed.spec_workspace_id().spec_bytes(),
            requested.spec_workspace_id().spec_bytes(),
        )
        && observed.spec_workspace_generation().spec_value()
            == requested.spec_workspace_generation().spec_value()
        && observed.spec_workspace_revision().spec_value()
            == requested.spec_workspace_revision().spec_value()
        && crate::revision::same_identifier(
            observed.spec_policy_id().spec_bytes(),
            requested.spec_policy_id().spec_bytes(),
        )
        && crate::revision::same_identifier(
            observed.spec_provider_profile_id().spec_bytes(),
            requested.spec_provider_profile_id().spec_bytes(),
        )
}

/// Logical aggregation of evaluator phase statuses.
///
/// This predicate only aggregates phase statuses. Each evaluator phase separately refines its
/// status against the contract, requested revision, and admitted evidence.
pub open spec fn acceptance_complete(
    contract_bound: bool,
    observations_fresh: bool,
    gates_complete: bool,
    evidence_complete: bool,
    reviews_complete: bool,
    blockers_complete: bool,
    approvals_complete: bool,
) -> bool {
    contract_bound
        && observations_fresh
        && gates_complete
        && evidence_complete
        && reviews_complete
        && blockers_complete
        && approvals_complete
}

/// Every passing gate observation for the requested revision is within its attempt budget.
pub open spec fn passing_gate_attempts_within_limit(
    observations: Seq<GateObservation>,
    requested: RevisionTuple,
    maximum: u16,
) -> bool {
    forall |index: int| 0 <= index < observations.len()
        && revision_fresh(#[trigger] observations[index].spec_revision(), requested)
        && observations[index].spec_outcome() == GateOutcome::Passed
        ==> observations[index].spec_attempt() <= maximum
}

/// Every review observation for the requested revision is within its cycle budget.
pub open spec fn review_cycles_within_limit(
    observations: Seq<ReviewObservation>,
    requested: RevisionTuple,
    maximum: u16,
) -> bool {
    forall |index: int| 0 <= index < observations.len()
        && revision_fresh(#[trigger] observations[index].spec_revision(), requested)
        ==> observations[index].spec_cycle_ordinal() <= maximum
}

} // verus!
