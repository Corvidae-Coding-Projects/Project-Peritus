//! Refinement bridges from executable evaluation stages to the total outcome model.

#![cfg(verus_only)]

use crate::{
    evaluation_outcome_model::{
        approval_conflicts_with_constraints, decision_is_exact, expected_denial_reason,
        preconstraint_denial_reason, reached_effective_scope,
    },
    AuthorityInstant, AuthorizationDenialReason, AuthorizationRequest, PolicyDecision,
    PolicyDecisionKind, PolicyDefinition,
};
use vstd::prelude::*;

verus! {

/// A denial built from the unchanged request scope refines an exact ordered denial reason.
pub proof fn requested_scope_denial_is_exact(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
    decision: &PolicyDecision,
    reason: AuthorizationDenialReason,
)
    requires
        expected_denial_reason(policy, request, observed_at) == Some(reason),
        !reached_effective_scope(policy, request, observed_at),
        decision.spec_kind() == PolicyDecisionKind::Denied,
        decision.spec_denial_reason() == Some(reason),
        decision.spec_scope_actor_id() == request.spec_actor_id(),
        decision.spec_scope_role() == request.spec_role(),
        decision.spec_scope_environment_id() == request.spec_environment_id(),
        decision.spec_scope_permissions() == request.spec_permissions(),
        decision.spec_scope_revision() == request.spec_revision(),
        decision.spec_scope_validity() == request.spec_validity(),
        decision.spec_scope_use_limit() == request.spec_use_limit(),
        decision.spec_evaluated_at() == observed_at,
    ensures decision_is_exact(policy, request, observed_at, decision),
{
}

/// The ordered preconstraint search selects the exact first denial.
pub proof fn preconstraint_denial_is_expected(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
    reason: AuthorizationDenialReason,
)
    requires
        preconstraint_denial_reason(policy, &request.spec_scope_value()) == Some(reason),
    ensures
        expected_denial_reason(policy, request, observed_at) == Some(reason),
        !reached_effective_scope(policy, request, observed_at),
{
}

/// An empty exact constraint fold selects the semantic empty-intersection denial.
pub proof fn empty_constraint_denial_is_expected(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
)
    requires
        preconstraint_denial_reason(policy, &request.spec_scope_value()).is_none(),
        crate::constraint_outcome_model::policy_constraint_outcome(
            policy,
            &request.spec_scope_value(),
        ).kind == 1,
    ensures
        expected_denial_reason(policy, request, observed_at)
            == Some(AuthorizationDenialReason::EmptyConstraintIntersection),
        !reached_effective_scope(policy, request, observed_at),
{
}

/// A conflicting exact approval fold selects the semantic approval-conflict denial.
pub proof fn approval_fold_denial_is_expected(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
)
    requires
        preconstraint_denial_reason(policy, &request.spec_scope_value()).is_none(),
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
        crate::approval_fold_model::policy_approval_values(
            policy,
            &request.spec_scope_value(),
        ).conflict,
    ensures
        expected_denial_reason(policy, request, observed_at)
            == Some(AuthorizationDenialReason::ApprovalConstraintConflict),
        !reached_effective_scope(policy, request, observed_at),
{
}

/// A failed effective validity check selects its exact before/expiry denial.
pub proof fn validity_denial_is_expected(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
    reason: AuthorizationDenialReason,
)
    requires
        preconstraint_denial_reason(policy, &request.spec_scope_value()).is_none(),
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
        !crate::approval_fold_model::policy_approval_values(
            policy,
            &request.spec_scope_value(),
        ).conflict,
        match reason {
            AuthorizationDenialReason::NotYetValid => {
                observed_at.spec_epoch()
                        == crate::constraint_outcome_model::policy_constraint_outcome(
                            policy,
                            &request.spec_scope_value(),
                        ).epoch
                    && observed_at.spec_tick_millis()
                        < crate::constraint_outcome_model::policy_constraint_outcome(
                            policy,
                            &request.spec_scope_value(),
                        ).not_before
            }
            AuthorizationDenialReason::Expired => {
                observed_at.spec_epoch()
                        == crate::constraint_outcome_model::policy_constraint_outcome(
                            policy,
                            &request.spec_scope_value(),
                        ).epoch
                    && observed_at.spec_tick_millis()
                        >= crate::constraint_outcome_model::policy_constraint_outcome(
                            policy,
                            &request.spec_scope_value(),
                        ).expires_at
            }
            _ => false,
        },
    ensures
        expected_denial_reason(policy, request, observed_at) == Some(reason),
        !reached_effective_scope(policy, request, observed_at),
{
}

/// A final effective-scope decision refines the exact approval and constraint folds.
pub proof fn effective_decision_is_exact(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    observed_at: AuthorityInstant,
    decision: &PolicyDecision,
)
    requires
        preconstraint_denial_reason(policy, &request.spec_scope_value()).is_none(),
        crate::constraint_outcome_model::policy_constraint_outcome(
            policy,
            &request.spec_scope_value(),
        ).kind == 0,
        !crate::approval_fold_model::policy_approval_values(
            policy,
            &request.spec_scope_value(),
        ).conflict,
        observed_at.spec_epoch()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).epoch,
        crate::constraint_outcome_model::policy_constraint_outcome(
            policy,
            &request.spec_scope_value(),
        ).not_before <= observed_at.spec_tick_millis(),
        observed_at.spec_tick_millis()
            < crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).expires_at,
        decision.spec_scope_actor_id() == request.spec_actor_id(),
        decision.spec_scope_role() == request.spec_role(),
        decision.spec_scope_environment_id() == request.spec_environment_id(),
        decision.spec_scope_permissions() == request.spec_permissions(),
        decision.spec_scope_revision() == request.spec_revision(),
        decision.spec_scope_validity().spec_not_before().spec_epoch()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).epoch,
        decision.spec_scope_validity().spec_expires_at().spec_epoch()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).expires_epoch,
        decision.spec_scope_validity().spec_not_before().spec_tick_millis()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).not_before,
        decision.spec_scope_validity().spec_expires_at().spec_tick_millis()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).expires_at,
        decision.spec_scope_use_limit().spec_remaining()
            == crate::constraint_outcome_model::policy_constraint_outcome(
                policy,
                &request.spec_scope_value(),
            ).uses,
        crate::approval_fold_model::decision_has_exact_approval(
            policy,
            &request.spec_scope_value(),
            decision,
        ),
        decision.spec_kind() == PolicyDecisionKind::Denied
            ==> decision.spec_denial_reason()
                == Some(AuthorizationDenialReason::ApprovalConstraintConflict),
        decision.spec_evaluated_at() == observed_at,
    ensures
        reached_effective_scope(policy, request, observed_at),
        decision_is_exact(policy, request, observed_at, decision),
{
    let constraints = crate::constraint_outcome_model::policy_constraint_outcome(
        policy,
        &request.spec_scope_value(),
    );
    let approvals = crate::approval_fold_model::policy_approval_values(
        policy,
        &request.spec_scope_value(),
    );
    assert(crate::approval_fold_model::effective_approval_conflict(
        approvals,
        decision.spec_scope_validity(),
    ) == approval_conflicts_with_constraints(approvals, constraints));
    decision.variant_agrees_with_spec_kind();
}

} // verus!
