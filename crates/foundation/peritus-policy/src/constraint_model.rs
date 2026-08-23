//! Constructive exact validity and use-limit intersection model.

#![cfg(verus_only)]

use crate::{CapabilityScope, CeilingGrant, PolicyDecision, PolicyDefinition};
use vstd::prelude::*;

verus! {

/// Folds every matching grant into exact `(not_before, expires_at, uses)` bounds.
pub open spec fn constraint_values_from(
    grants: Seq<CeilingGrant>,
    scope: &CapabilityScope,
    index: nat,
    not_before: int,
    expires_at: int,
    uses: Option<int>,
) -> (int, int, Option<int>)
    decreases grants.len() - index,
{
    if index >= grants.len() {
        (not_before, expires_at, uses)
    } else if grants[index as int].spec_matches_scope(scope) {
        constraint_values_from(
            grants,
            scope,
            index + 1,
            crate::model::maximum_int(
                not_before,
                grants[index as int].spec_validity().spec_not_before().spec_tick_millis(),
            ),
            crate::model::minimum_int(
                expires_at,
                grants[index as int].spec_validity().spec_expires_at().spec_tick_millis(),
            ),
            crate::model::minimum_use_limit(
                uses,
                grants[index as int].spec_use_limit().spec_remaining(),
            ),
        )
    } else {
        constraint_values_from(grants, scope, index + 1, not_before, expires_at, uses)
    }
}

/// Returns the exact whole-request effective validity and logical-use bounds.
pub open spec fn effective_constraint_values(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> (int, int, Option<int>) {
    constraint_values_from(
        policy.spec_grants(),
        scope,
        0,
        crate::model::maximum_int(
            scope.spec_validity().spec_not_before().spec_tick_millis(),
            policy.spec_boundary_validity().spec_not_before().spec_tick_millis(),
        ),
        crate::model::minimum_int(
            scope.spec_validity().spec_expires_at().spec_tick_millis(),
            policy.spec_boundary_validity().spec_expires_at().spec_tick_millis(),
        ),
        crate::model::minimum_use_limit(
            scope.spec_use_limit().spec_remaining(),
            policy.spec_boundary_use_limit().spec_remaining(),
        ),
    )
}

/// Relates a non-denied decision to the exact executable constraint fold.
pub open spec fn decision_has_exact_constraints(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
    decision: &PolicyDecision,
) -> bool {
    let values = effective_constraint_values(policy, scope);
    decision.spec_scope_validity().spec_not_before().spec_tick_millis() == values.0
        && decision.spec_scope_validity().spec_expires_at().spec_tick_millis() == values.1
        && decision.spec_scope_use_limit().spec_remaining() == values.2
}

} // verus!
