//! Composable whole-request policy-evaluation reducers.

use crate::{
    AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState, AuthorizationDenialReason,
    AuthorizationRequest, CapabilityScope, PolicyDecision, PolicyDefinition, PolicyError,
    ValidityWindow,
    evaluation_approval::{RestrictionResult, approval_conjunction},
    evaluation_constraints::{ConstraintResult, EffectiveConstraints, grant_constraints},
    evaluation_result::{exact_requested_denial, finalize_decision},
};

enum ValidityResult {
    Accepted,
    Denied(AuthorizationDenialReason),
    Failed(PolicyError),
}
const fn validity_denial(
    validity: ValidityWindow,
    observed_at: AuthorityInstant,
) -> ValidityResult {
    let contains = match validity.contains(observed_at) {
        Ok(value) => value,
        Err(error) => return ValidityResult::Failed(error),
    };
    if contains {
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
) -> Option<AuthorizationDenialReason> {
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
) -> Result<PolicyDecision, AuthorityTimeFailure> {
    let requested = request.scope();
    let approval = match approval_conjunction(policy, requested) {
        Ok(RestrictionResult::Accepted(value)) => value,
        Ok(RestrictionResult::Denied(reason)) => {
            return exact_requested_denial(policy, request, time_state, observed_at, reason);
        }
        Err(error) => return Err(AuthorityTimeFailure::new(error, time_state)),
    };
    match validity_denial(constraints.validity, observed_at) {
        ValidityResult::Accepted => {}
        ValidityResult::Failed(error) => {
            return Err(AuthorityTimeFailure::new(error, time_state));
        }
        ValidityResult::Denied(reason) => {
            return exact_requested_denial(policy, request, time_state, observed_at, reason);
        }
    }
    let result = finalize_decision(
        policy,
        request.into_scope(),
        constraints.validity,
        constraints.use_limit,
        approval,
        time_state,
        observed_at,
    );
    result
}
pub fn evaluate_definition(
    policy: &PolicyDefinition,
    request: AuthorizationRequest,
    time_state: AuthorityTimeState,
    observed_at: AuthorityInstant,
) -> Result<PolicyDecision, AuthorityTimeFailure> {
    match time_state.validate_observation(observed_at) {
        Ok(()) => {}
        Err(error) => return Err(AuthorityTimeFailure::new(error, time_state)),
    }
    let requested = request.scope();
    if let Some(reason) = preconstraint_denial(policy, requested) {
        return exact_requested_denial(policy, request, time_state, observed_at, reason);
    }
    let constraint_result = grant_constraints(policy, requested);
    let constraints = match constraint_result {
        ConstraintResult::Accepted(value) => value,
        ConstraintResult::Denied(reason) => {
            return exact_requested_denial(policy, request, time_state, observed_at, reason);
        }
    };
    evaluate_with_constraints(policy, request, time_state, observed_at, constraints)
}
