//! Composable whole-request policy-evaluation reducers.

use crate::{
    evaluation_approval::{approval_conjunction, RestrictionResult},
    evaluation_constraints::{grant_constraints, ConstraintResult, EffectiveConstraints},
    evaluation_result::{exact_requested_denial, finalize_decision}, AuthorityInstant,
    AuthorityTimeFailure, AuthorityTimeState, AuthorizationDenialReason, AuthorizationRequest,
    CapabilityScope, PolicyDecision, PolicyDefinition, PolicyError, ValidityWindow,
};
use vstd::prelude::*;

verus! {

enum ValidityResult {
    Accepted,
    Denied(AuthorizationDenialReason),
    Failed(PolicyError),
}

const fn validity_denial(
    validity: ValidityWindow,
    observed_at: AuthorityInstant,
) -> (result: ValidityResult)
    ensures
        match result {
            ValidityResult::Accepted => validity.spec_contains(observed_at),
            ValidityResult::Denied(AuthorizationDenialReason::NotYetValid) => {
                observed_at.spec_epoch() == validity.spec_not_before().spec_epoch()
                    && observed_at.spec_tick_millis()
                        < validity.spec_not_before().spec_tick_millis()
            }
            ValidityResult::Denied(AuthorizationDenialReason::Expired) => {
                observed_at.spec_epoch() == validity.spec_not_before().spec_epoch()
                    && observed_at.spec_tick_millis()
                        >= validity.spec_expires_at().spec_tick_millis()
            }
            ValidityResult::Denied(_) => false,
            ValidityResult::Failed(error) => {
                observed_at.spec_epoch() != validity.spec_not_before().spec_epoch()
                    && error.spec_kind() == crate::PolicyErrorKind::ClockEpochMismatch
            }
        },
{
    let contains = match validity.contains(observed_at) {
        Ok(value) => value,
        Err(error) => return ValidityResult::Failed(error),
    };
    if contains {
        assert(validity.spec_contains(observed_at));
        return ValidityResult::Accepted;
    }
    if observed_at.tick_millis() < validity.not_before().tick_millis() {
        ValidityResult::Denied(AuthorizationDenialReason::NotYetValid)
    } else {
        ValidityResult::Denied(AuthorizationDenialReason::Expired)
    }
}

fn preconstraint_denial(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> (reason: Option<AuthorizationDenialReason>)
    ensures
        reason == crate::evaluation_outcome_model::preconstraint_denial_reason(
            policy,
            requested,
        ),
{
    if !policy.matches_policy_id(requested) {
        Some(AuthorizationDenialReason::PolicyMismatch)
    } else if !policy.boundary_contains(requested) {
        Some(AuthorizationDenialReason::OutsideAuthorityBoundary)
    } else if let Some(reason) = policy.first_operation_denial(requested) {
        Some(reason)
    } else if policy.has_immutable_deny(requested) {
        Some(AuthorizationDenialReason::ImmutableDeny)
    } else if policy.has_restriction_deny(requested) {
        Some(AuthorizationDenialReason::RestrictionDeny)
    } else if !policy.has_full_coverage(requested) {
        Some(AuthorizationDenialReason::IncompleteCeilingCoverage)
    } else {
        None
    }
}

fn evaluate_with_constraints(
    policy: &PolicyDefinition,
    request: AuthorizationRequest,
    time_state: AuthorityTimeState,
    observed_at: AuthorityInstant,
    constraints: EffectiveConstraints,
) -> (result: Result<PolicyDecision, AuthorityTimeFailure>)
    requires
        time_state.spec_accepts(observed_at),
        observed_at.spec_epoch() == time_state.spec_epoch(),
        observed_at.spec_tick_millis() >= time_state.spec_greatest_tick_millis(),
        crate::evaluation_outcome_model::preconstraint_denial_reason(
            policy,
            &request.spec_scope_value(),
        ).is_none(),
        crate::constraint_outcome_model::policy_constraint_outcome(
            policy,
            &request.spec_scope_value(),
        ).kind == 0,
        crate::constraint_outcome_model::policy_constraint_outcome(
            policy,
            &request.spec_scope_value(),
        ).not_before < crate::constraint_outcome_model::policy_constraint_outcome(
            policy,
            &request.spec_scope_value(),
        ).expires_at,
        constraints.validity.spec_not_before().spec_epoch()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).epoch,
        constraints.validity.spec_expires_at().spec_epoch()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).expires_epoch,
        constraints.validity.spec_not_before().spec_tick_millis()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).not_before,
        constraints.validity.spec_expires_at().spec_tick_millis()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).expires_at,
        constraints.use_limit.spec_remaining()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).uses,
        constraints.validity.spec_not_before().spec_tick_millis()
            == crate::constraint_model::effective_constraint_values(
                policy,
                &request.spec_scope_value(),
            ).0,
        constraints.validity.spec_expires_at().spec_tick_millis()
            == crate::constraint_model::effective_constraint_values(
                policy,
                &request.spec_scope_value(),
            ).1,
        constraints.use_limit.spec_remaining()
            == crate::constraint_model::effective_constraint_values(
                policy,
                &request.spec_scope_value(),
            ).2,
    ensures
        crate::evaluation_outcome_model::evaluation_result_is_exact(
            policy,
            &request,
            time_state,
            observed_at,
            &result,
        ),
        match result {
            Ok(decision) => crate::model::policy_evaluation_safety(
                policy,
                &request,
                time_state,
                observed_at,
                &decision,
            ),
            Err(_) => true,
        },
{
    let requested = request.scope();
    let approval = match approval_conjunction(policy, requested) {
        Ok(RestrictionResult::Accepted(value)) => value,
        Ok(RestrictionResult::Denied(reason)) => {
            proof {
                crate::evaluation_outcome_proofs::approval_fold_denial_is_expected(
                    policy,
                    &request,
                    observed_at,
                );
            }
            return exact_requested_denial(
                policy,
                request,
                time_state,
                observed_at,
                reason,
            );
        }
        Err(error) => return Err(AuthorityTimeFailure::new(error, time_state)),
    };
    match validity_denial(constraints.validity, observed_at) {
        ValidityResult::Accepted => {},
        ValidityResult::Failed(error) => {
            assert(crate::evaluation_outcome_model::expected_evaluation_error(
                policy,
                &request,
                time_state,
                observed_at,
            ) == Some(error.spec_kind()));
            return Err(AuthorityTimeFailure::new(error, time_state));
        }
        ValidityResult::Denied(reason) => {
            proof {
                crate::evaluation_outcome_proofs::validity_denial_is_expected(
                    policy,
                    &request,
                    observed_at,
                    reason,
                );
            }
            return exact_requested_denial(
                policy,
                request,
                time_state,
                observed_at,
                reason,
            );
        }
    }
    assert(crate::approval_fold_model::approval_accumulator_values(&approval)
        == crate::approval_fold_model::policy_approval_values(
            policy,
            &request.spec_scope_value(),
        ));
    let result = finalize_decision(
        policy,
        request.into_scope(),
        constraints.validity,
        constraints.use_limit,
        approval,
        time_state,
        observed_at,
    );
    proof {
        if let Ok(decision) = &result {
            crate::evaluation_outcome_proofs::effective_decision_is_exact(
                policy,
                &request,
                observed_at,
                decision,
            );
            crate::evaluation_result::establish_evaluation_safety(
                policy,
                &request,
                time_state,
                observed_at,
                decision,
            );
        }
    }
    result
}

pub fn evaluate_definition(
    policy: &PolicyDefinition,
    request: AuthorizationRequest,
    time_state: AuthorityTimeState,
    observed_at: AuthorityInstant,
) -> (result: Result<PolicyDecision, AuthorityTimeFailure>)
    ensures
        crate::evaluation_outcome_model::evaluation_result_is_exact(
            policy,
            &request,
            time_state,
            observed_at,
            &result,
        ),
        match result {
            Ok(decision) => {
                crate::model::policy_evaluation_safety(
                    policy,
                    &request,
                    time_state,
                    observed_at,
                    &decision,
                )
                    && decision.spec_scope_actor_id() == request.spec_actor_id()
                    && decision.spec_scope_role() == request.spec_role()
                    && decision.spec_scope_environment_id() == request.spec_environment_id()
                    && decision.spec_scope_permissions() == request.spec_permissions()
                    && decision.spec_scope_revision() == request.spec_revision()
                    && decision.spec_evaluated_at() == observed_at
            }
            Err(failure) => {
                failure.spec_epoch() == time_state.spec_epoch()
                    && failure.spec_greatest_tick_millis()
                        == time_state.spec_greatest_tick_millis()
            }
        },
{
    match time_state.validate_observation(observed_at) {
        Ok(()) => {}
        Err(error) => return Err(AuthorityTimeFailure::new(error, time_state)),
    }
    let requested = request.scope();
    if let Some(reason) = preconstraint_denial(policy, requested) {
        proof {
            crate::evaluation_outcome_proofs::preconstraint_denial_is_expected(
                policy,
                &request,
                observed_at,
                reason,
            );
        }
        return exact_requested_denial(
            policy,
            request,
            time_state,
            observed_at,
            reason,
        );
    }
    let constraint_result = grant_constraints(policy, requested);
    let constraints = match constraint_result {
        ConstraintResult::Accepted(value) => value,
        ConstraintResult::Denied(reason) => {
            proof {
                crate::evaluation_outcome_proofs::empty_constraint_denial_is_expected(
                    policy,
                    &request,
                    observed_at,
                );
            }
            return exact_requested_denial(
                policy,
                request,
                time_state,
                observed_at,
                reason,
            );
        }
    };
    evaluate_with_constraints(policy, request, time_state, observed_at, constraints)
}

} // verus!
