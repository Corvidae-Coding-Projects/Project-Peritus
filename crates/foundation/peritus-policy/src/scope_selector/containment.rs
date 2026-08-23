//! Ordered exact parent-boundary containment for complete selectors.

use super::ScopeSelector;
use crate::{ScopeDimension, identity::revision_values_equal};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn actor_mismatch(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> bool {
    match selector.actors.0 {
        crate::selector::SelectorValues::AnyWithinParent => false,
        crate::selector::SelectorValues::Exact(values) => exists |index: int|
            0 <= index < values@.len() && !boundary.spec_contains_actor(values@[index]),
    }
}

pub(crate) open spec fn role_mismatch(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> bool {
    match selector.roles.0 {
        crate::selector::SelectorValues::AnyWithinParent => false,
        crate::selector::SelectorValues::Exact(values) => exists |index: int|
            0 <= index < values@.len() && !boundary.spec_contains_role(values@[index]),
    }
}

pub(crate) open spec fn environment_mismatch(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> bool {
    match selector.environments.0 {
        crate::selector::SelectorValues::AnyWithinParent => false,
        crate::selector::SelectorValues::Exact(values) => exists |index: int|
            0 <= index < values@.len() && !boundary.spec_contains_environment(values@[index]),
    }
}

pub(crate) open spec fn permission_mismatch(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> bool {
    match selector.permissions.exact {
        Some(values) => exists |index: int|
            0 <= index < values.spec_values().len()
                && !boundary.spec_contains_permission(&values.spec_values()[index]),
        None => false,
    }
}

impl ScopeSelector {
    /// Returns the first parent-containment mismatch in executable validation order.
    pub(crate) open spec fn spec_first_boundary_mismatch(
        &self,
        boundary: &crate::AuthorityBoundary,
    ) -> Option<ScopeDimension> {
        if !crate::model::same_revision(self.revision, boundary.spec_revision()) {
            Some(ScopeDimension::Revision)
        } else if actor_mismatch(self, boundary) {
            Some(ScopeDimension::Actor)
        } else if role_mismatch(self, boundary) {
            Some(ScopeDimension::Role)
        } else if environment_mismatch(self, boundary) {
            Some(ScopeDimension::Environment)
        } else if permission_mismatch(self, boundary) {
            Some(ScopeDimension::Permissions)
        } else {
            None
        }
    }

    pub(crate) fn first_boundary_mismatch(
        &self,
        boundary: &crate::AuthorityBoundary,
    ) -> (mismatch: Option<ScopeDimension>)
        ensures mismatch == self.spec_first_boundary_mismatch(boundary),
    {
        if !revision_values_equal(self.revision, *boundary.revision()) {
            return Some(ScopeDimension::Revision);
        }
        if actors_outside(self, boundary) {
            return Some(ScopeDimension::Actor);
        }
        if roles_outside(self, boundary) {
            return Some(ScopeDimension::Role);
        }
        if environments_outside(self, boundary) {
            return Some(ScopeDimension::Environment);
        }
        if permissions_outside(self, boundary) {
            return Some(ScopeDimension::Permissions);
        }
        None
    }
}

fn actors_outside(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> (outside: bool)
    ensures outside == actor_mismatch(selector, boundary),
{
    reveal(actor_mismatch);
    let Some(values) = selector.actors.exact_values_checked() else { return false; };
    assert(selector.actors.spec_exact_values_internal() == Some(values@));
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            selector.actors.spec_exact_values_internal() == Some(values@),
            forall |prior: int| 0 <= prior < index ==>
                boundary.spec_contains_actor(values@[prior]),
        decreases values.len() - index,
    {
        if !boundary.contains_actor(values[index]) {
            assert(exists |witness: int| 0 <= witness < values@.len()
                && !boundary.spec_contains_actor(values@[witness])) by {
                let witness = index as int;
                assert(0 <= witness < values@.len());
                assert(!boundary.spec_contains_actor(values@[witness]));
            }
            assert(actor_mismatch(selector, boundary)) by {
                reveal(actor_mismatch);
                assert(selector.actors.spec_exact_values_internal() == Some(values@));
                assert(exists |witness: int| 0 <= witness < values@.len()
                    && !boundary.spec_contains_actor(values@[witness]));
            }
            return true;
        }
        index += 1;
    }
    false
}

fn roles_outside(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> (outside: bool)
    ensures outside == role_mismatch(selector, boundary),
{
    reveal(role_mismatch);
    let Some(values) = selector.roles.exact_values_checked() else { return false; };
    assert(selector.roles.spec_exact_values_internal() == Some(values@));
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            selector.roles.spec_exact_values_internal() == Some(values@),
            forall |prior: int| 0 <= prior < index ==>
                boundary.spec_contains_role(values@[prior]),
        decreases values.len() - index,
    {
        if !boundary.contains_role(values[index]) {
            assert(exists |witness: int| 0 <= witness < values@.len()
                && !boundary.spec_contains_role(values@[witness])) by {
                let witness = index as int;
                assert(0 <= witness < values@.len());
                assert(!boundary.spec_contains_role(values@[witness]));
            }
            assert(role_mismatch(selector, boundary)) by {
                reveal(role_mismatch);
                assert(selector.roles.spec_exact_values_internal() == Some(values@));
                assert(exists |witness: int| 0 <= witness < values@.len()
                    && !boundary.spec_contains_role(values@[witness]));
            }
            return true;
        }
        index += 1;
    }
    false
}

fn environments_outside(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> (outside: bool)
    ensures outside == environment_mismatch(selector, boundary),
{
    reveal(environment_mismatch);
    let Some(values) = selector.environments.exact_values_checked() else { return false; };
    assert(selector.environments.spec_exact_values_internal() == Some(values@));
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            selector.environments.spec_exact_values_internal() == Some(values@),
            forall |prior: int| 0 <= prior < index ==>
                boundary.spec_contains_environment(values@[prior]),
        decreases values.len() - index,
    {
        if !boundary.contains_environment(values[index]) {
            assert(exists |witness: int| 0 <= witness < values@.len()
                && !boundary.spec_contains_environment(values@[witness])) by {
                let witness = index as int;
                assert(0 <= witness < values@.len());
                assert(!boundary.spec_contains_environment(values@[witness]));
            }
            assert(environment_mismatch(selector, boundary)) by {
                reveal(environment_mismatch);
                assert(selector.environments.spec_exact_values_internal() == Some(values@));
                assert(exists |witness: int| 0 <= witness < values@.len()
                    && !boundary.spec_contains_environment(values@[witness]));
            }
            return true;
        }
        index += 1;
    }
    false
}

fn permissions_outside(
    selector: &ScopeSelector,
    boundary: &crate::AuthorityBoundary,
) -> (outside: bool)
    ensures outside == permission_mismatch(selector, boundary),
{
    reveal(permission_mismatch);
    let Some(values) = selector.permissions.exact_values_internal() else { return false; };
    assert(selector.permissions.spec_exact_values_internal() == Some(values.spec_values()));
    let values = values.as_slice();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            selector.permissions.spec_exact_values_internal() == Some(values@),
            forall |prior: int| 0 <= prior < index ==>
                boundary.spec_contains_permission(&values@[prior]),
        decreases values.len() - index,
    {
        if !boundary.contains_permission(&values[index]) {
            assert(exists |witness: int| 0 <= witness < values@.len()
                && !boundary.spec_contains_permission(&values@[witness])) by {
                let witness = index as int;
                assert(0 <= witness < values@.len());
                assert(!boundary.spec_contains_permission(&values@[witness]));
            }
            assert(permission_mismatch(selector, boundary)) by {
                reveal(permission_mismatch);
                assert(selector.permissions.spec_exact_values_internal()
                    == Some(values@));
                assert(exists |witness: int| 0 <= witness < values@.len()
                    && !boundary.spec_contains_permission(&values@[witness]));
            }
            return true;
        }
        index += 1;
    }
    false
}

} // verus!
