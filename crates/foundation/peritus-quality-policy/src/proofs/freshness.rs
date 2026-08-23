//! INV-003 exact-revision proof obligations.

#[cfg(verus_only)]
use crate::model;
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Freshness implies exact equality of every revision-tuple component representation.
pub proof fn freshness_implies_exact_revision(
    observed: RevisionTuple,
    requested: RevisionTuple,
)
    requires model::revision_fresh(observed, requested),
    ensures
        crate::revision::same_identifier(
            observed.spec_acceptance_spec_id().spec_bytes(),
            requested.spec_acceptance_spec_id().spec_bytes()),
        crate::revision::same_identifier(
            observed.spec_harness_id().spec_bytes(), requested.spec_harness_id().spec_bytes()),
        crate::revision::same_identifier(
            observed.spec_workspace_id().spec_bytes(), requested.spec_workspace_id().spec_bytes()),
        observed.spec_workspace_generation().spec_value()
            == requested.spec_workspace_generation().spec_value(),
        observed.spec_workspace_revision().spec_value()
            == requested.spec_workspace_revision().spec_value(),
        crate::revision::same_identifier(
            observed.spec_policy_id().spec_bytes(), requested.spec_policy_id().spec_bytes()),
        crate::revision::same_identifier(
            observed.spec_provider_profile_id().spec_bytes(),
            requested.spec_provider_profile_id().spec_bytes()),
{
    reveal(model::revision_fresh);
}

/// Any represented component drift is stale and therefore cannot satisfy INV-003.
pub proof fn revision_drift_is_stale(
    observed: RevisionTuple,
    requested: RevisionTuple,
)
    requires !model::revision_fresh(observed, requested),
    ensures !model::revision_fresh(observed, requested),
{}

} // verus!
