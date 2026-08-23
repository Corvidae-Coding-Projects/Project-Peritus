//! Mathematical acceptance and freshness predicates.

#[cfg(verus_only)]
use crate::{GateObservation, GateOutcome, ReviewObservation};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

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
/// This predicate deliberately does not claim a refinement to contract collections whose
/// specification views are not exported by `peritus-spec`. Concrete predicates below refine the
/// exact-revision and completion-limit checks over typed observations.
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
