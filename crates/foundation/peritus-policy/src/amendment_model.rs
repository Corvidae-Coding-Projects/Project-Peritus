//! Exact constructive model for a single-tier restriction amendment.

#![cfg(verus_only)]

use crate::{PolicyTier, RestrictionLayer};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Returns whether only the policy identity changed in a successor revision tuple.
pub open spec fn revision_is_exact_successor(
    base: RevisionTuple,
    successor: RevisionTuple,
    successor_policy_id: [u8; 16],
) -> bool {
    successor.spec_acceptance_spec_id() == base.spec_acceptance_spec_id()
        && successor.spec_harness_id() == base.spec_harness_id()
        && successor.spec_workspace_id() == base.spec_workspace_id()
        && successor.spec_workspace_generation() == base.spec_workspace_generation()
        && successor.spec_workspace_revision() == base.spec_workspace_revision()
        && successor.spec_policy_id().spec_bytes() == successor_policy_id
        && successor.spec_provider_profile_id() == base.spec_provider_profile_id()
}

/// Relates a complete successor suffix to the exact single-tier amendment algorithm.
pub open spec fn exact_amended_layers_from(
    base: Seq<RestrictionLayer>,
    successor: Seq<RestrictionLayer>,
    target: PolicyTier,
    replacement: &RestrictionLayer,
    revision: RevisionTuple,
    base_index: nat,
    inserted: bool,
) -> bool
    decreases
        base.len() - base_index,
        if inserted { 0nat } else { 1nat },
{
    if base_index >= base.len() {
        if inserted {
            successor.len() == 0
        } else {
            successor.len() == 1
                && successor[0].spec_is_revision_rebind_of(replacement, revision)
        }
    } else if !inserted && base[base_index as int].spec_tier().spec_rank()
        >= target.spec_rank()
    {
        successor.len() > 0
            && successor[0].spec_is_revision_rebind_of(replacement, revision)
            && exact_amended_layers_from(
                base,
                successor.subrange(1, successor.len() as int),
                target,
                replacement,
                revision,
                base_index,
                true,
            )
    } else if base[base_index as int].spec_tier().spec_rank() == target.spec_rank() {
        exact_amended_layers_from(
            base,
            successor,
            target,
            replacement,
            revision,
            base_index + 1,
            inserted,
        )
    } else {
        successor.len() > 0
            && successor[0].spec_is_revision_rebind_of(
                &base[base_index as int],
                revision,
            )
            && exact_amended_layers_from(
                base,
                successor.subrange(1, successor.len() as int),
                target,
                replacement,
                revision,
                base_index + 1,
                inserted,
            )
    }
}

} // verus!
