//! Construction of final policy decisions after all policy folds succeed.

use crate::{
    ApprovalRequirement, AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState,
    AuthorizationDenial, AuthorizationDenialReason, AuthorizationRequest,
    CapabilityIssuancePlan, CapabilityScope, PolicyDecision, PolicyDefinition, UseLimit,
    ValidityWindow,
};
#[cfg(verus_only)]
use crate::approval_fold_model as approvals;
use vstd::prelude::*;

verus! {

pub const fn denial(
    reason: AuthorizationDenialReason,
    scope: CapabilityScope,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
) -> (decision: PolicyDecision)
    ensures
        matches!(decision, PolicyDecision::Denied(_)),
        decision.spec_kind() == crate::PolicyDecisionKind::Denied,
        decision.spec_denial_reason() == Some(reason),
        decision.spec_scope_actor_id() == scope.spec_actor_id(),
        decision.spec_scope_role() == scope.spec_role(),
        decision.spec_scope_environment_id() == scope.spec_environment_id(),
        decision.spec_scope_permissions() == scope.spec_permissions(),
        decision.spec_scope_revision() == scope.spec_revision(),
        decision.spec_scope_validity() == scope.spec_validity(),
        decision.spec_scope_use_limit() == scope.spec_use_limit(),
        decision.spec_evaluated_at() == evaluated_at,
        decision.spec_time_epoch() == time_state.spec_epoch(),
        decision.spec_greatest_tick() == time_state.spec_greatest_tick_millis(),
{
    PolicyDecision::denied(AuthorizationDenial::new(
        reason,
        scope,
        evaluated_at,
        time_state,
    ))
}

pub fn exact_requested_denial(
    _policy: &PolicyDefinition,
    request: AuthorizationRequest,
    previous_time: AuthorityTimeState,
    observed_at: AuthorityInstant,
    reason: AuthorizationDenialReason,
) -> (result: Result<PolicyDecision, AuthorityTimeFailure>)
    ensures
        match result {
            Ok(decision) => {
                decision.spec_kind() == crate::PolicyDecisionKind::Denied
                    && decision.spec_denial_reason() == Some(reason)
                    && decision.spec_scope_actor_id() == request.spec_actor_id()
                    && decision.spec_scope_role() == request.spec_role()
                    && decision.spec_scope_environment_id()
                        == request.spec_environment_id()
                    && decision.spec_scope_permissions() == request.spec_permissions()
                    && decision.spec_scope_revision() == request.spec_revision()
                    && decision.spec_scope_validity() == request.spec_validity()
                    && decision.spec_scope_use_limit() == request.spec_use_limit()
                    && decision.spec_evaluated_at() == observed_at
                    && decision.spec_time_epoch() == observed_at.spec_epoch()
                    && decision.spec_greatest_tick() == observed_at.spec_tick_millis()
                    && (crate::evaluation_outcome_model::expected_denial_reason(
                        _policy,
                        &request,
                        observed_at,
                    ) == Some(reason)
                        && !crate::evaluation_outcome_model::reached_effective_scope(
                            _policy,
                            &request,
                            observed_at,
                        ) ==> {
                            crate::evaluation_outcome_model::expected_evaluation_error(
                                _policy,
                                &request,
                                previous_time,
                                observed_at,
                            ).is_none()
                                && crate::evaluation_outcome_model::decision_is_exact(
                                    _policy,
                                    &request,
                                    observed_at,
                                    &decision,
                                )
                        })
            }
            Err(failure) => {
                !previous_time.spec_accepts(observed_at)
                    && failure.spec_epoch() == previous_time.spec_epoch()
                    && failure.spec_greatest_tick_millis()
                        == previous_time.spec_greatest_tick_millis()
            }
        },
{
    let next_time_state = previous_time.observe(observed_at)?;
    let scope = request.into_scope();
    let decision = denial(reason, scope, observed_at, next_time_state);
    proof {
        if crate::evaluation_outcome_model::expected_denial_reason(
            _policy,
            &request,
            observed_at,
        ) == Some(reason) && !crate::evaluation_outcome_model::reached_effective_scope(
            _policy,
            &request,
            observed_at,
        ) {
            crate::evaluation_outcome_proofs::requested_scope_denial_is_exact(
                _policy,
                &request,
                observed_at,
                &decision,
                reason,
            );
        }
    }
    Ok(decision)
}

pub open spec fn finalization_inputs_are_exact(
    policy: &PolicyDefinition,
    requested_scope: &CapabilityScope,
    effective_validity: ValidityWindow,
    effective_use_limit: UseLimit,
    approval: &Option<ApprovalRequirement>,
) -> bool {
    effective_validity.spec_not_before().spec_tick_millis()
            == crate::constraint_model::effective_constraint_values(
                policy,
                requested_scope,
            ).0
        && effective_validity.spec_expires_at().spec_tick_millis()
            == crate::constraint_model::effective_constraint_values(
                policy,
                requested_scope,
            ).1
        && effective_use_limit.spec_remaining()
            == crate::constraint_model::effective_constraint_values(
                policy,
                requested_scope,
            ).2
        && approvals::approval_accumulator_values(approval)
            == approvals::policy_approval_values(policy, requested_scope)
}

proof fn establish_empty_approval_values(
    policy: &PolicyDefinition,
    requested_scope: &CapabilityScope,
    effective_validity: ValidityWindow,
    effective_use_limit: UseLimit,
)
    requires
        finalization_inputs_are_exact(
            policy,
            requested_scope,
            effective_validity,
            effective_use_limit,
            &None,
        ),
    ensures
        !approvals::policy_approval_values(policy, requested_scope).required,
        !approvals::policy_approval_values(policy, requested_scope).conflict,
{
}

pub open spec fn finalization_result_is_exact(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
    validity: ValidityWindow,
    use_limit: UseLimit,
    approval: &Option<ApprovalRequirement>,
    time: AuthorityTimeState,
    observed: AuthorityInstant,
    result: &Result<PolicyDecision, AuthorityTimeFailure>,
) -> bool {
    (time.spec_accepts(observed) ==> result.is_ok())
        && match result {
            Ok(decision) => {
                decision.spec_scope_actor_id() == requested.spec_actor_id()
                    && decision.spec_scope_role() == requested.spec_role()
                    && decision.spec_scope_environment_id()
                        == requested.spec_environment_id()
                    && decision.spec_scope_permissions() == requested.spec_permissions()
                    && decision.spec_scope_revision() == requested.spec_revision()
                    && decision.spec_scope_validity() == validity
                    && decision.spec_scope_use_limit() == use_limit
                    && decision.spec_evaluated_at() == observed
                    && decision.spec_time_epoch() == observed.spec_epoch()
                    && decision.spec_greatest_tick() == observed.spec_tick_millis()
                    && (decision.spec_kind() == crate::PolicyDecisionKind::Denied
                        ==> decision.spec_denial_reason()
                            == Some(AuthorizationDenialReason::ApprovalConstraintConflict))
                    && (finalization_inputs_are_exact(
                        policy,
                        requested,
                        validity,
                        use_limit,
                        approval,
                    ) ==> {
                        approvals::decision_has_exact_approval(policy, requested, &decision)
                            && crate::constraint_model::decision_has_exact_constraints(
                                policy,
                                requested,
                                &decision,
                            )
                    })
            }
            Err(failure) => {
                failure.spec_epoch() == time.spec_epoch()
                    && failure.spec_greatest_tick_millis()
                        == time.spec_greatest_tick_millis()
            }
        }
}

pub fn finalize_decision(
    policy: &PolicyDefinition,
    requested: CapabilityScope,
    validity: ValidityWindow,
    use_limit: UseLimit,
    approval: Option<ApprovalRequirement>,
    time: AuthorityTimeState,
    observed: AuthorityInstant,
) -> (result: Result<PolicyDecision, AuthorityTimeFailure>)
    requires policy.spec_first_operation_denial(&requested).is_none(),
    ensures finalization_result_is_exact(
        policy,
        &requested,
        validity,
        use_limit,
        &approval,
        time,
        observed,
        &result,
    ),
{
    let ghost exact_inputs = finalization_inputs_are_exact(
        policy,
        &requested,
        validity,
        use_limit,
        &approval,
    );
    let Some(requirement) = approval else {
        proof {
            if exact_inputs {
                establish_empty_approval_values(
                    policy,
                    &requested,
                    validity,
                    use_limit,
                );
            }
        }
        let effective_scope = requested.with_constraints(validity, use_limit);
        let next_time_state = time.observe(observed)?;
        let decision = PolicyDecision::authorized(CapabilityIssuancePlan::new(
            effective_scope,
            observed,
            next_time_state,
        ));
        proof {
            if exact_inputs {
                assert(approvals::decision_has_exact_approval(
                    policy,
                    &requested,
                    &decision,
                ));
            }
        }
        return Ok(decision);
    };
    crate::evaluation_approval_result::finalize_required_approval(
        policy,
        requested,
        validity,
        use_limit,
        &requirement,
        time,
        observed,
    )
}

pub(crate) proof fn establish_evaluation_safety(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    previous_time: AuthorityTimeState,
    observed_at: AuthorityInstant,
    decision: &PolicyDecision,
)
    requires
        policy.spec_matches_policy_id(&request.spec_scope_value()),
        policy.spec_boundary_contains(&request.spec_scope_value()),
        policy.spec_first_operation_denial(&request.spec_scope_value()).is_none(),
        !policy.spec_has_immutable_deny(&request.spec_scope_value()),
        !policy.spec_has_restriction_deny(&request.spec_scope_value()),
        policy.spec_has_full_coverage(&request.spec_scope_value()),
        crate::constraint_model::decision_has_exact_constraints(
            policy,
            &request.spec_scope_value(),
            decision,
        ),
        approvals::decision_has_exact_approval(
            policy,
            &request.spec_scope_value(),
            decision,
        ),
        decision.spec_evaluated_at() == observed_at,
        decision.spec_time_epoch() == observed_at.spec_epoch(),
        decision.spec_greatest_tick() == observed_at.spec_tick_millis(),
        observed_at.spec_epoch() == previous_time.spec_epoch(),
        observed_at.spec_tick_millis() >= previous_time.spec_greatest_tick_millis(),
        decision.spec_scope_validity().spec_contains(observed_at),
    ensures
        crate::model::policy_evaluation_safety(
            policy,
            request,
            previous_time,
            observed_at,
            decision,
        ),
{
    let approval_values = approvals::policy_approval_values(policy, &request.spec_scope_value());
    let constrained_conflict =
        approvals::effective_approval_conflict(approval_values, decision.spec_scope_validity());
    decision.variant_agrees_with_spec_kind();
    assert((approval_values.conflict || (approval_values.required && constrained_conflict))
        ==> decision.spec_kind() == crate::PolicyDecisionKind::Denied);
    crate::proofs::evaluator_cannot_broaden_allowed_queries(
        policy,
        request,
        decision,
    );
}

} // verus!
