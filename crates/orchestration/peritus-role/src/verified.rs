//! Executable facts used by C6 proof roots and ordinary callers.

use crate::RoleProfile;
use peritus_policy::OperationClass;
use vstd::prelude::*;

verus! {

/// Returns whether every operation in the role projection remains B1-permitted.
#[must_use]
pub fn capability_view_is_narrow(profile: &RoleProfile) -> (result: bool)
{
    profile.capabilities().is_narrow()
}

/// Returns whether the canonical reviewer context satisfies the C6 freshness boundary.
#[must_use]
pub fn reviewer_context_is_fresh(profile: &RoleProfile) -> bool {
    profile.actor_role() == peritus_policy::ActorRole::Reviewer
        && profile.context().requires_fresh_context()
        && !profile.context().allows_producer_ancestry()
        && !profile.capabilities().permits(OperationClass::WorkspaceMutation)
}

} // verus!
