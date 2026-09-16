//! Executable facts used by C6 proof roots and ordinary callers.

use crate::RoleProfile;
use peritus_policy::OperationClass;
use vstd::prelude::*;

verus! {

/// Returns whether every operation in the role projection remains B1-permitted.
#[must_use]
pub fn capability_view_is_narrow(profile: &RoleProfile) -> (result: bool)
    ensures result == profile.spec_capabilities().spec_is_narrow(),
{
    profile.capabilities().is_narrow()
}

/// Returns whether the canonical reviewer context satisfies the C6 freshness boundary.
#[must_use]
pub fn reviewer_context_is_fresh(profile: &RoleProfile) -> (fresh: bool)
    ensures fresh == (profile.spec_actor_role() == peritus_policy::ActorRole::Reviewer
        && profile.spec_context().spec_fresh_context()
        && !profile.spec_context().spec_allow_producer_ancestry()
        && !profile.spec_capabilities().spec_operations().contains(OperationClass::WorkspaceMutation)),
        profile.spec_for_role(profile.spec_actor_role()) ==>
            fresh == (profile.spec_actor_role() == peritus_policy::ActorRole::Reviewer),
{
    matches!(profile.actor_role(), peritus_policy::ActorRole::Reviewer)
        && profile.context().requires_fresh_context()
        && !profile.context().allows_producer_ancestry()
        && !profile.capabilities().permits(OperationClass::WorkspaceMutation)
}

} // verus!
