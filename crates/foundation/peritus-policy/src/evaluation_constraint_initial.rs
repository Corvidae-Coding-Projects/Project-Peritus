//! Initial request-to-ceiling constraint intersection.

use crate::{
    AuthorizationDenialReason, CapabilityScope, PolicyDefinition, ValidityWindow,
    evaluation_constraints::{ConstraintResult, EffectiveConstraints},
};

pub const fn intersect_window(
    left: ValidityWindow,
    right: ValidityWindow,
) -> Option<ValidityWindow> {
    match left.intersection(right) {
        Ok(value) => Some(value),
        Err(_error) => {
            let left_epoch = left.not_before().epoch().get();
            let right_epoch = right.not_before().epoch().get();
            if left_epoch != right_epoch {
                return None;
            }
            None
        }
    }
}
pub const fn initial_constraints(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> ConstraintResult {
    let use_limit = requested.use_limit().intersection(policy.ceiling().boundary().use_limit());
    let validity = intersect_window(requested.validity(), policy.ceiling().boundary().validity());
    let Some(validity) = validity else {
        return ConstraintResult::Denied(AuthorizationDenialReason::EmptyConstraintIntersection);
    };
    ConstraintResult::Accepted(EffectiveConstraints { validity, use_limit })
}
