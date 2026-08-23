//! Total exact outcome model for validity and logical-use intersection.

#![cfg(verus_only)]

use crate::{CapabilityScope, CeilingGrant, PolicyDefinition, UseLimit, ValidityWindow};
use vstd::prelude::*;

verus! {

/// Exact total state of the constraint fold.
pub struct ConstraintOutcome {
    /// `0` for accepted and `1` for any empty intersection, including an epoch conflict.
    pub kind: int,
    /// Active validity epoch for an accepted fold.
    pub epoch: int,
    /// Epoch selected for the exclusive bound.
    pub expires_epoch: int,
    /// Inclusive effective tick for an accepted fold.
    pub not_before: int,
    /// Exclusive effective tick for an accepted fold.
    pub expires_at: int,
    /// Exact effective logical-use bound.
    pub uses: Option<int>,
}

/// Returns whether the fold has an accepted nonempty validity interval.
pub open spec fn constraint_accepted(outcome: ConstraintOutcome) -> bool {
    outcome.kind == 0
}

/// Returns whether the fold found an empty or cross-epoch validity intersection.
pub open spec fn constraint_empty(outcome: ConstraintOutcome) -> bool {
    outcome.kind == 1
}

/// Projects concrete accepted reducer values into the total outcome model.
pub open spec fn accepted_constraint_outcome(
    validity: ValidityWindow,
    use_limit: UseLimit,
) -> ConstraintOutcome {
    ConstraintOutcome {
        kind: 0,
        epoch: validity.spec_not_before().spec_epoch(),
        expires_epoch: validity.spec_expires_at().spec_epoch(),
        not_before: validity.spec_not_before().spec_tick_millis(),
        expires_at: validity.spec_expires_at().spec_tick_millis(),
        uses: use_limit.spec_remaining(),
    }
}

/// Intersects one exact validity/use constraint into a total prior outcome.
pub open spec fn intersect_constraint_outcome(
    prior: ConstraintOutcome,
    validity: ValidityWindow,
    use_limit: UseLimit,
) -> ConstraintOutcome {
    if prior.kind != 0 {
        prior
    } else if prior.epoch != validity.spec_not_before().spec_epoch() {
        ConstraintOutcome {
            kind: 1,
            epoch: prior.epoch,
            expires_epoch: prior.expires_epoch,
            not_before: prior.not_before,
            expires_at: prior.expires_at,
            uses: prior.uses,
        }
    } else {
        let not_before = crate::model::maximum_int(
            prior.not_before,
            validity.spec_not_before().spec_tick_millis(),
        );
        let expires_at = crate::model::minimum_int(
            prior.expires_at,
            validity.spec_expires_at().spec_tick_millis(),
        );
        let not_before_epoch = if prior.not_before
            >= validity.spec_not_before().spec_tick_millis()
        {
            prior.epoch
        } else {
            validity.spec_not_before().spec_epoch()
        };
        let expires_epoch = if prior.expires_at
            <= validity.spec_expires_at().spec_tick_millis()
        {
            prior.expires_epoch
        } else {
            validity.spec_expires_at().spec_epoch()
        };
        ConstraintOutcome {
            kind: if not_before_epoch == expires_epoch && not_before < expires_at {
                0
            } else {
                1
            },
            epoch: not_before_epoch,
            expires_epoch,
            not_before,
            expires_at,
            uses: crate::model::minimum_use_limit(
                prior.uses,
                use_limit.spec_remaining(),
            ),
        }
    }
}

/// Returns the first total request/parent-boundary constraint outcome.
pub open spec fn initial_constraint_outcome(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> ConstraintOutcome {
    let requested = scope.spec_validity();
    let initial = ConstraintOutcome {
        kind: 0,
        epoch: requested.spec_not_before().spec_epoch(),
        expires_epoch: requested.spec_expires_at().spec_epoch(),
        not_before: requested.spec_not_before().spec_tick_millis(),
        expires_at: requested.spec_expires_at().spec_tick_millis(),
        uses: scope.spec_use_limit().spec_remaining(),
    };
    intersect_constraint_outcome(
        initial,
        policy.spec_boundary_validity(),
        policy.spec_boundary_use_limit(),
    )
}

/// Folds every remaining matching ceiling grant into a total outcome.
pub open spec fn constraint_outcome_from(
    grants: Seq<CeilingGrant>,
    scope: &CapabilityScope,
    index: nat,
    accumulated: ConstraintOutcome,
) -> ConstraintOutcome
    decreases grants.len() - index,
{
    if index >= grants.len() || accumulated.kind != 0 {
        accumulated
    } else {
        let next = if grants[index as int].spec_matches_scope(scope) {
            intersect_constraint_outcome(
                accumulated,
                grants[index as int].spec_validity(),
                grants[index as int].spec_use_limit(),
            )
        } else {
            accumulated
        };
        constraint_outcome_from(grants, scope, index + 1, next)
    }
}

/// Returns the exact total constraint outcome for one complete request.
pub open spec fn policy_constraint_outcome(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> ConstraintOutcome {
    constraint_outcome_from(
        policy.spec_grants(),
        scope,
        0,
        initial_constraint_outcome(policy, scope),
    )
}

} // verus!
