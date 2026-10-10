//! Exact executable validity and use-limit intersection reducer.

use crate::{
    AuthorizationDenialReason, CapabilityScope, PolicyDefinition, UseLimit, ValidityWindow,
    evaluation_constraint_initial::{initial_constraints, intersect_window},
};

#[doc = " Complete effective constraints after every matching ceiling grant."]
#[derive(Clone, Copy)]
pub struct EffectiveConstraints {
    #[doc = " Exact nonempty effective validity interval."]
    pub validity: ValidityWindow,
    #[doc = " Exact minimum logical-use bound."]
    pub use_limit: UseLimit,
}
#[doc = " Total semantic result of reducing policy constraints."]
pub enum ConstraintResult {
    #[doc = " Every constraint intersects to this exact effective value."]
    Accepted(EffectiveConstraints),
    #[doc = " The checked constraints form an empty intersection."]
    Denied(AuthorizationDenialReason),
}
fn fold_grants(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
    initial: EffectiveConstraints,
) -> ConstraintResult {
    let mut validity = initial.validity;
    let mut use_limit = initial.use_limit;
    let grants = policy.ceiling().grants();
    let mut grant_index = 0;
    while grant_index < grants.len() {
        let grant = &grants[grant_index];
        if grant.matches_scope(requested) {
            let next_validity = intersect_window(validity, grant.validity());
            let Some(next_validity) = next_validity else {
                return ConstraintResult::Denied(
                    AuthorizationDenialReason::EmptyConstraintIntersection,
                );
            };
            validity = next_validity;
            use_limit = use_limit.intersection(grant.use_limit());
        }
        grant_index += 1;
    }
    ConstraintResult::Accepted(EffectiveConstraints { validity, use_limit })
}
#[doc = " Reduces every applicable validity and use constraint into one total semantic result."]
pub fn grant_constraints(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> ConstraintResult {
    match initial_constraints(policy, requested) {
        ConstraintResult::Denied(reason) => ConstraintResult::Denied(reason),
        ConstraintResult::Accepted(initial) => fold_grants(policy, requested, initial),
    }
}
