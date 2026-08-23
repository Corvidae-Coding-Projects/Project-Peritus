//! Initial request-to-ceiling constraint intersection.

use crate::{
    evaluation_constraints::{ConstraintResult, EffectiveConstraints},
    AuthorizationDenialReason, CapabilityScope, PolicyDefinition, ValidityWindow,
};
use vstd::prelude::*;

verus! {

pub const fn intersect_window(
    left: ValidityWindow,
    right: ValidityWindow,
) -> (result: Option<ValidityWindow>)
    ensures
        match result {
            Some(value) => {
                !crate::approval_model::window_intersection_conflict(left, right)
                    && value.spec_not_before().spec_epoch()
                        == value.spec_expires_at().spec_epoch()
                    && value.spec_not_before().spec_tick_millis()
                        < value.spec_expires_at().spec_tick_millis()
                    && value.spec_not_before().spec_epoch()
                        == crate::approval_model::intersection_not_before_epoch(left, right)
                    && value.spec_expires_at().spec_epoch()
                        == crate::approval_model::intersection_expires_epoch(left, right)
                    && value.spec_not_before().spec_tick_millis()
                        == crate::model::maximum_int(
                            left.spec_not_before().spec_tick_millis(),
                            right.spec_not_before().spec_tick_millis(),
                        )
                    && value.spec_expires_at().spec_tick_millis()
                        == crate::model::minimum_int(
                            left.spec_expires_at().spec_tick_millis(),
                            right.spec_expires_at().spec_tick_millis(),
                        )
            }
            None => crate::approval_model::window_intersection_conflict(left, right),
        },
{
    match left.intersection(right) {
        Ok(value) => {
            assert(!crate::approval_model::window_intersection_conflict(left, right));
            assert(value.spec_not_before().spec_epoch()
                == value.spec_expires_at().spec_epoch());
            assert(value.spec_not_before().spec_tick_millis()
                < value.spec_expires_at().spec_tick_millis());
            assert(value.spec_not_before().spec_epoch()
                == crate::approval_model::intersection_not_before_epoch(left, right));
            assert(value.spec_expires_at().spec_epoch()
                == crate::approval_model::intersection_expires_epoch(left, right));
            assert(value.spec_not_before().spec_tick_millis()
                == crate::model::maximum_int(
                    left.spec_not_before().spec_tick_millis(),
                    right.spec_not_before().spec_tick_millis(),
                ));
            assert(value.spec_expires_at().spec_tick_millis()
                == crate::model::minimum_int(
                    left.spec_expires_at().spec_tick_millis(),
                    right.spec_expires_at().spec_tick_millis(),
                ));
            Some(value)
        }
        Err(_error) => {
            let left_epoch = left.not_before().epoch().get();
            let right_epoch = right.not_before().epoch().get();
            if left_epoch != right_epoch {
                assert(left.spec_not_before().spec_epoch()
                    != right.spec_not_before().spec_epoch());
                assert(crate::approval_model::window_intersection_conflict(left, right));
                return None;
            }
            assert(left.spec_not_before().spec_epoch()
                == right.spec_not_before().spec_epoch());
            assert(crate::approval_model::window_intersection_conflict(left, right));
            None
        }
    }
}

pub const fn initial_constraints(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> (result: ConstraintResult)
    ensures
        match result {
            ConstraintResult::Accepted(value) => {
                let exact = crate::constraint_outcome_model::initial_constraint_outcome(
                    policy,
                    requested,
                );
                exact.kind == 0
                    && crate::constraint_outcome_model::accepted_constraint_outcome(
                        value.validity,
                        value.use_limit,
                    ) == exact
                    && value.validity.spec_not_before().spec_tick_millis()
                        == crate::model::maximum_int(
                            requested.spec_validity().spec_not_before().spec_tick_millis(),
                            policy.spec_boundary_validity().spec_not_before().spec_tick_millis(),
                        )
                    && value.validity.spec_expires_at().spec_tick_millis()
                        == crate::model::minimum_int(
                            requested.spec_validity().spec_expires_at().spec_tick_millis(),
                            policy.spec_boundary_validity().spec_expires_at().spec_tick_millis(),
                        )
                    && value.use_limit.spec_remaining()
                        == crate::model::minimum_use_limit(
                            requested.spec_use_limit().spec_remaining(),
                            policy.spec_boundary_use_limit().spec_remaining(),
                        )
            }
            ConstraintResult::Denied(reason) => {
                reason == AuthorizationDenialReason::EmptyConstraintIntersection
                    && crate::constraint_outcome_model::initial_constraint_outcome(
                        policy,
                        requested,
                    ).kind == 1
            }
        },
{
    let use_limit = requested
        .use_limit()
        .intersection(policy.ceiling().boundary().use_limit());
    let validity = intersect_window(
        requested.validity(),
        policy.ceiling().boundary().validity(),
    );
    let Some(validity) = validity else {
        assert(crate::constraint_outcome_model::initial_constraint_outcome(
            policy,
            requested,
        ).kind == 1);
        return ConstraintResult::Denied(
            AuthorizationDenialReason::EmptyConstraintIntersection,
        );
    };
    assert(crate::constraint_outcome_model::accepted_constraint_outcome(
        validity,
        use_limit,
    ) == crate::constraint_outcome_model::initial_constraint_outcome(policy, requested));
    ConstraintResult::Accepted(EffectiveConstraints { validity, use_limit })
}

} // verus!
