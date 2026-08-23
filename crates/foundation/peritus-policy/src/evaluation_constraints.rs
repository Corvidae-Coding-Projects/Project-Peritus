//! Exact executable validity and use-limit intersection reducer.

use crate::{
    evaluation_constraint_initial::{initial_constraints, intersect_window},
    AuthorizationDenialReason, CapabilityScope, PolicyDefinition, UseLimit, ValidityWindow,
};
use vstd::prelude::*;

verus! {

/// Complete effective constraints after every matching ceiling grant.
#[derive(Clone, Copy)]
pub struct EffectiveConstraints {
    /// Exact nonempty effective validity interval.
    pub validity: ValidityWindow,
    /// Exact minimum logical-use bound.
    pub use_limit: UseLimit,
}

/// Total semantic result of reducing policy constraints.
pub enum ConstraintResult {
    /// Every constraint intersects to this exact effective value.
    Accepted(EffectiveConstraints),
    /// The checked constraints form an empty intersection.
    Denied(AuthorizationDenialReason),
}

proof fn terminal_constraint_is_exact(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
    grant_index: nat,
    previous: crate::constraint_outcome_model::ConstraintOutcome,
    terminal: crate::constraint_outcome_model::ConstraintOutcome,
)
    requires
        grant_index < policy.spec_grants().len(),
        policy.spec_grants()[grant_index as int].spec_matches_scope(requested),
        crate::constraint_outcome_model::constraint_outcome_from(
            policy.spec_grants(),
            requested,
            grant_index,
            previous,
        ) == crate::constraint_outcome_model::policy_constraint_outcome(policy, requested),
        terminal == crate::constraint_outcome_model::intersect_constraint_outcome(
            previous,
            policy.spec_grants()[grant_index as int].spec_validity(),
            policy.spec_grants()[grant_index as int].spec_use_limit(),
        ),
        terminal.kind != 0,
    ensures
        crate::constraint_outcome_model::policy_constraint_outcome(policy, requested)
            == terminal,
{
    assert(crate::constraint_outcome_model::constraint_outcome_from(
        policy.spec_grants(),
        requested,
        grant_index,
        previous,
    ) == crate::constraint_outcome_model::constraint_outcome_from(
        policy.spec_grants(),
        requested,
        grant_index + 1,
        terminal,
    ));
    assert(crate::constraint_outcome_model::constraint_outcome_from(
        policy.spec_grants(),
        requested,
        grant_index + 1,
        terminal,
    ) == terminal);
}

fn fold_grants(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
    initial: EffectiveConstraints,
) -> (result: ConstraintResult)
    requires
        crate::constraint_outcome_model::accepted_constraint_outcome(
            initial.validity,
            initial.use_limit,
        ) == crate::constraint_outcome_model::initial_constraint_outcome(policy, requested),
        initial.validity.spec_not_before().spec_tick_millis()
            == crate::model::maximum_int(
                requested.spec_validity().spec_not_before().spec_tick_millis(),
                policy.spec_boundary_validity().spec_not_before().spec_tick_millis(),
            ),
        initial.validity.spec_expires_at().spec_tick_millis()
            == crate::model::minimum_int(
                requested.spec_validity().spec_expires_at().spec_tick_millis(),
                policy.spec_boundary_validity().spec_expires_at().spec_tick_millis(),
            ),
        initial.use_limit.spec_remaining()
            == crate::model::minimum_use_limit(
                requested.spec_use_limit().spec_remaining(),
                policy.spec_boundary_use_limit().spec_remaining(),
            ),
    ensures
        match result {
            ConstraintResult::Accepted(value) => {
                let exact = crate::constraint_outcome_model::policy_constraint_outcome(
                    policy,
                    requested,
                );
                exact.kind == 0
                    && exact.not_before < exact.expires_at
                    && crate::constraint_outcome_model::accepted_constraint_outcome(
                        value.validity,
                        value.use_limit,
                    ) == exact
                    && value.validity.spec_not_before().spec_tick_millis()
                        == crate::constraint_model::effective_constraint_values(
                            policy,
                            requested,
                        ).0
                    && value.validity.spec_expires_at().spec_tick_millis()
                        == crate::constraint_model::effective_constraint_values(
                            policy,
                            requested,
                        ).1
                    && value.use_limit.spec_remaining()
                        == crate::constraint_model::effective_constraint_values(
                            policy,
                            requested,
                        ).2
            }
            ConstraintResult::Denied(reason) => {
                reason == AuthorizationDenialReason::EmptyConstraintIntersection
                    && crate::constraint_outcome_model::policy_constraint_outcome(
                        policy,
                        requested,
                    ).kind == 1
            }
        },
{
    let mut validity = initial.validity;
    let mut use_limit = initial.use_limit;
    let grants = policy.ceiling().grants();
    assert(crate::constraint_outcome_model::constraint_outcome_from(
        grants@,
        requested,
        0,
        crate::constraint_outcome_model::accepted_constraint_outcome(validity, use_limit),
    ) == crate::constraint_outcome_model::policy_constraint_outcome(policy, requested));
    let mut grant_index = 0;
    while grant_index < grants.len()
        invariant
            0 <= grant_index <= grants.len(),
            grants@ == policy.spec_grants(),
            crate::constraint_model::constraint_values_from(
                grants@,
                requested,
                grant_index as nat,
                validity.spec_not_before().spec_tick_millis(),
                validity.spec_expires_at().spec_tick_millis(),
                use_limit.spec_remaining(),
            ) == crate::constraint_model::effective_constraint_values(policy, requested),
            crate::constraint_outcome_model::constraint_outcome_from(
                grants@,
                requested,
                grant_index as nat,
                crate::constraint_outcome_model::accepted_constraint_outcome(
                    validity,
                    use_limit,
                ),
            ) == crate::constraint_outcome_model::policy_constraint_outcome(policy, requested),
            validity.spec_not_before().spec_epoch()
                == validity.spec_expires_at().spec_epoch(),
            validity.spec_not_before().spec_tick_millis()
                < validity.spec_expires_at().spec_tick_millis(),
        decreases grants.len() - grant_index,
    {
        let grant = &grants[grant_index];
        let ghost previous_validity = validity;
        let ghost previous_use_limit = use_limit;
        if grant.matches_scope(requested) {
            let next_validity = intersect_window(validity, grant.validity());
            let Some(next_validity) = next_validity else {
                let ghost previous = crate::constraint_outcome_model::accepted_constraint_outcome(
                    previous_validity,
                    previous_use_limit,
                );
                let ghost terminal = crate::constraint_outcome_model::intersect_constraint_outcome(
                    previous,
                    grant.spec_validity(),
                    grant.spec_use_limit(),
                );
                assert(terminal.kind == 1);
                proof {
                    terminal_constraint_is_exact(
                        policy,
                        requested,
                        grant_index as nat,
                        previous,
                        terminal,
                    );
                }
                return ConstraintResult::Denied(
                    AuthorizationDenialReason::EmptyConstraintIntersection,
                );
            };
            validity = next_validity;
            use_limit = use_limit.intersection(grant.use_limit());
            assert(crate::constraint_outcome_model::accepted_constraint_outcome(
                validity,
                use_limit,
            ) == crate::constraint_outcome_model::intersect_constraint_outcome(
                crate::constraint_outcome_model::accepted_constraint_outcome(
                    previous_validity,
                    previous_use_limit,
                ),
                grant.spec_validity(),
                grant.spec_use_limit(),
            ));
        }
        assert(crate::constraint_model::constraint_values_from(
            grants@,
            requested,
            grant_index as nat,
            previous_validity.spec_not_before().spec_tick_millis(),
            previous_validity.spec_expires_at().spec_tick_millis(),
            previous_use_limit.spec_remaining(),
        ) == crate::constraint_model::constraint_values_from(
            grants@,
            requested,
            (grant_index + 1) as nat,
            validity.spec_not_before().spec_tick_millis(),
            validity.spec_expires_at().spec_tick_millis(),
            use_limit.spec_remaining(),
        ));
        grant_index += 1;
    }
    ConstraintResult::Accepted(EffectiveConstraints { validity, use_limit })
}

/// Reduces every applicable validity and use constraint into one total semantic result.
pub fn grant_constraints(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> (result: ConstraintResult)
    ensures
        match result {
            ConstraintResult::Accepted(value) => {
                let exact = crate::constraint_outcome_model::policy_constraint_outcome(
                    policy,
                    requested,
                );
                exact.kind == 0
                    && exact.not_before < exact.expires_at
                    && value.validity.spec_not_before().spec_epoch() == exact.epoch
                    && value.validity.spec_expires_at().spec_epoch() == exact.expires_epoch
                    && value.validity.spec_not_before().spec_tick_millis() == exact.not_before
                    && value.validity.spec_expires_at().spec_tick_millis() == exact.expires_at
                    && value.use_limit.spec_remaining() == exact.uses
                    && value.validity.spec_not_before().spec_tick_millis()
                        == crate::constraint_model::effective_constraint_values(
                            policy,
                            requested,
                        ).0
                    && value.validity.spec_expires_at().spec_tick_millis()
                        == crate::constraint_model::effective_constraint_values(
                            policy,
                            requested,
                        ).1
                    && value.use_limit.spec_remaining()
                        == crate::constraint_model::effective_constraint_values(
                            policy,
                            requested,
                        ).2
            }
            ConstraintResult::Denied(reason) => {
                reason == AuthorizationDenialReason::EmptyConstraintIntersection
                    && crate::constraint_outcome_model::policy_constraint_outcome(
                        policy,
                        requested,
                    ).kind == 1
            }
        },
{
    match initial_constraints(policy, requested) {
        ConstraintResult::Denied(reason) => {
            assert(crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                requested,
            ).kind == 1);
            ConstraintResult::Denied(reason)
        }
        ConstraintResult::Accepted(initial) => {
            assert(initial.validity.spec_not_before().spec_tick_millis()
                == crate::model::maximum_int(
                    requested.spec_validity().spec_not_before().spec_tick_millis(),
                    policy.spec_boundary_validity().spec_not_before().spec_tick_millis(),
                ));
            assert(initial.validity.spec_expires_at().spec_tick_millis()
                == crate::model::minimum_int(
                    requested.spec_validity().spec_expires_at().spec_tick_millis(),
                    policy.spec_boundary_validity().spec_expires_at().spec_tick_millis(),
                ));
            assert(initial.use_limit.spec_remaining()
                == crate::model::minimum_use_limit(
                    requested.spec_use_limit().spec_remaining(),
                    policy.spec_boundary_use_limit().spec_remaining(),
                ));
            fold_grants(policy, requested, initial)
        }
    }
}

} // verus!
