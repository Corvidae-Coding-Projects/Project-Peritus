//! Total ordered reference outcome for whole-request policy evaluation.

#![cfg(verus_only)]

use crate::{
    AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState, AuthorizationDenialReason,
    AuthorizationRequest, CapabilityScope, PolicyDecision, PolicyDecisionKind, PolicyDefinition,
    PolicyErrorKind,
};
use vstd::prelude::*;

verus! {

/// Returns the exact first policy denial before effective-constraint evaluation.
pub open spec fn preconstraint_denial_reason(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> Option<AuthorizationDenialReason> {
    if !policy.spec_matches_policy_id(scope) {
        Some(AuthorizationDenialReason::PolicyMismatch)
    } else if !policy.spec_boundary_contains(scope) {
        Some(AuthorizationDenialReason::OutsideAuthorityBoundary)
    } else if policy.spec_first_operation_denial(scope).is_some() {
        policy.spec_first_operation_denial(scope)
    } else if policy.spec_has_immutable_deny(scope) {
        Some(AuthorizationDenialReason::ImmutableDeny)
    } else if policy.spec_has_restriction_deny(scope) {
        Some(AuthorizationDenialReason::RestrictionDeny)
    } else if !policy.spec_has_full_coverage(scope) {
        Some(AuthorizationDenialReason::IncompleteCeilingCoverage)
    } else {
        None
    }
}

/// Returns whether exact approval validity conflicts with the effective constraint outcome.
pub open spec fn approval_conflicts_with_constraints(
    approvals: crate::approval_fold_model::ApprovalValues,
    constraints: crate::constraint_outcome_model::ConstraintOutcome,
) -> bool {
    let next_not_before_epoch = if approvals.not_before_tick >= constraints.not_before {
        approvals.not_before_epoch
    } else {
        constraints.epoch
    };
    let next_expires_epoch = if approvals.expires_tick <= constraints.expires_at {
        approvals.expires_epoch
    } else {
        constraints.expires_epoch
    };
    approvals.not_before_epoch != constraints.epoch
        || next_not_before_epoch != next_expires_epoch
        || crate::model::maximum_int(approvals.not_before_tick, constraints.not_before)
            >= crate::model::minimum_int(approvals.expires_tick, constraints.expires_at)
}

/// Returns the exact typed failure expected before any policy decision can be produced.
pub open spec fn expected_evaluation_error(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    previous_time: AuthorityTimeState,
    observed_at: AuthorityInstant,
) -> Option<PolicyErrorKind> {
    let scope = request.spec_scope_value();
    let constraints = crate::constraint_outcome_model::policy_constraint_outcome(policy, &scope);
    let approvals = crate::approval_fold_model::policy_approval_values(policy, &scope);
    if observed_at.spec_epoch() != previous_time.spec_epoch() {
        Some(PolicyErrorKind::ClockEpochMismatch)
    } else if observed_at.spec_tick_millis() < previous_time.spec_greatest_tick_millis() {
        Some(PolicyErrorKind::ClockRegression)
    } else if preconstraint_denial_reason(policy, &scope).is_some() {
        None
    } else if constraints.kind == 0 && !approvals.conflict
        && observed_at.spec_epoch() != constraints.epoch
    {
        Some(PolicyErrorKind::ClockEpochMismatch)
    } else {
        None
    }
}

/// Returns the exact stable denial reason after all ordered policy checks.
pub open spec fn expected_denial_reason(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
) -> Option<AuthorizationDenialReason> {
    let scope = request.spec_scope_value();
    let prior = preconstraint_denial_reason(policy, &scope);
    let constraints = crate::constraint_outcome_model::policy_constraint_outcome(policy, &scope);
    let approvals = crate::approval_fold_model::policy_approval_values(policy, &scope);
    if prior.is_some() {
        prior
    } else if constraints.kind == 1 {
        Some(AuthorizationDenialReason::EmptyConstraintIntersection)
    } else if approvals.conflict {
        Some(AuthorizationDenialReason::ApprovalConstraintConflict)
    } else if observed_at.spec_epoch() != constraints.epoch {
        None
    } else if observed_at.spec_tick_millis() < constraints.not_before {
        Some(AuthorizationDenialReason::NotYetValid)
    } else if observed_at.spec_tick_millis() >= constraints.expires_at {
        Some(AuthorizationDenialReason::Expired)
    } else if approvals.required
        && approval_conflicts_with_constraints(approvals, constraints)
    {
        Some(AuthorizationDenialReason::ApprovalConstraintConflict)
    } else {
        None
    }
}

/// Returns whether evaluation reached final effective-scope decision construction.
pub open spec fn reached_effective_scope(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
) -> bool {
    let scope = request.spec_scope_value();
    let constraints = crate::constraint_outcome_model::policy_constraint_outcome(policy, &scope);
    let approvals = crate::approval_fold_model::policy_approval_values(policy, &scope);
    preconstraint_denial_reason(policy, &scope).is_none()
        && constraints.kind == 0
        && !approvals.conflict
        && observed_at.spec_epoch() == constraints.epoch
        && constraints.not_before <= observed_at.spec_tick_millis()
        && observed_at.spec_tick_millis() < constraints.expires_at
}

/// Relates one decision to the unique exact ordered evaluator result.
pub open spec fn decision_is_exact(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
    decision: &PolicyDecision,
) -> bool {
    let scope = request.spec_scope_value();
    let constraints = crate::constraint_outcome_model::policy_constraint_outcome(policy, &scope);
    let approvals = crate::approval_fold_model::policy_approval_values(policy, &scope);
    let expected_denial = expected_denial_reason(policy, request, observed_at);
    let effective = reached_effective_scope(policy, request, observed_at);
    decision.spec_scope_actor_id() == request.spec_actor_id()
        && decision.spec_scope_role() == request.spec_role()
        && decision.spec_scope_environment_id() == request.spec_environment_id()
        && decision.spec_scope_permissions() == request.spec_permissions()
        && decision.spec_scope_revision() == request.spec_revision()
        && decision.spec_evaluated_at() == observed_at
        && (match expected_denial {
            Some(reason) => {
                decision.spec_kind() == PolicyDecisionKind::Denied
                    && decision.spec_denial_reason() == Some(reason)
            }
            None => {
                if approvals.required {
                    decision.spec_kind() == PolicyDecisionKind::ApprovalRequired
                } else {
                    decision.spec_kind() == PolicyDecisionKind::Authorized
                }
            }
        })
        && (if effective {
            decision.spec_scope_validity().spec_not_before().spec_epoch() == constraints.epoch
                && decision.spec_scope_validity().spec_expires_at().spec_epoch()
                    == constraints.expires_epoch
                && decision.spec_scope_validity().spec_not_before().spec_tick_millis()
                    == constraints.not_before
                && decision.spec_scope_validity().spec_expires_at().spec_tick_millis()
                    == constraints.expires_at
                && decision.spec_scope_use_limit().spec_remaining() == constraints.uses
                && crate::approval_fold_model::decision_has_exact_approval(
                    policy,
                    &scope,
                    decision,
                )
        } else {
            decision.spec_scope_validity() == request.spec_validity()
                && decision.spec_scope_use_limit() == request.spec_use_limit()
        })
}

/// Relates the public value-in/value-out reducer result to its unique exact outcome.
pub open spec fn evaluation_result_is_exact(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    previous_time: AuthorityTimeState,
    observed_at: AuthorityInstant,
    result: &Result<PolicyDecision, AuthorityTimeFailure>,
) -> bool {
    let expected_error = expected_evaluation_error(
        policy,
        request,
        previous_time,
        observed_at,
    );
    match result {
        Ok(decision) => {
            expected_error.is_none()
                && decision_is_exact(policy, request, observed_at, decision)
                && decision.spec_time_epoch() == observed_at.spec_epoch()
                && decision.spec_greatest_tick() == observed_at.spec_tick_millis()
        }
        Err(failure) => {
            expected_error == Some(failure.spec_error_kind())
                && failure.spec_epoch() == previous_time.spec_epoch()
                && failure.spec_greatest_tick_millis()
                    == previous_time.spec_greatest_tick_millis()
        }
    }
}

} // verus!
