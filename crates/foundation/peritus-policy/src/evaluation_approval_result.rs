//! Final decision construction when policy evaluation requires approval.

use crate::{
    ApprovalRequirement, AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState,
    AuthorizationDenialReason, CapabilityScope, EscalationChallenge, PolicyDecision,
    PolicyDefinition, UseLimit, ValidityWindow,
};
#[cfg(verus_only)]
use crate::approval_fold_model as approvals;
use vstd::prelude::*;

verus! {

pub fn finalize_required_approval(
    policy: &PolicyDefinition,
    requested: CapabilityScope,
    validity: ValidityWindow,
    use_limit: UseLimit,
    requirement: &ApprovalRequirement,
    time: AuthorityTimeState,
    observed: AuthorityInstant,
) -> (result: Result<PolicyDecision, AuthorityTimeFailure>)
    requires policy.spec_first_operation_denial(&requested).is_none(),
    ensures crate::evaluation_result::finalization_result_is_exact(
        policy,
        &requested,
        validity,
        use_limit,
        &Some(*requirement),
        time,
        observed,
        &result,
    ),
{
    let ghost exact_inputs = crate::evaluation_result::finalization_inputs_are_exact(
        policy,
        &requested,
        validity,
        use_limit,
        &Some(*requirement),
    );
    let ghost exact_approval = approvals::policy_approval_values(policy, &requested);
    proof {
        if exact_inputs {
            assert(approvals::approval_values_from_requirement(requirement) == exact_approval);
            assert(exact_approval.required);
            assert(!exact_approval.conflict);
        }
    }
    let effective_scope = requested.with_constraints(validity, use_limit);
    let ghost original_requirement = *requirement;
    let constrained = match requirement.constrain_validity(effective_scope.validity()) {
        Ok(result) => result,
        Err(error) => return Err(AuthorityTimeFailure::new(error, time)),
    };
    let next_time_state = time.observe(observed)?;
    proof {
        policy.operation_denial_depends_only_on_role_permissions(
            &effective_scope,
            &requested,
        );
    }
    if let Some(requirement) = constrained {
        let risks = policy.mandatory_risks_for_scope(&effective_scope);
        let challenge = EscalationChallenge::new(
            effective_scope,
            requirement,
            risks,
            observed,
            next_time_state,
        );
        let decision = PolicyDecision::approval_required(challenge);
        proof {
            assert(decision.spec_scope_permissions() == requested.spec_permissions());
            policy.mandatory_risks_depend_only_on_permissions(
                &effective_scope,
                &requested,
            );
            if exact_inputs {
                assert(!approvals::effective_approval_conflict(
                    exact_approval,
                    decision.spec_scope_validity(),
                ));
                assert(approvals::decision_has_exact_approval(
                    policy,
                    &requested,
                    &decision,
                ));
            }
        }
        Ok(decision)
    } else {
        proof {
            if exact_inputs {
                approvals::constrained_none_implies_effective_conflict(
                    &original_requirement,
                    effective_scope.spec_validity(),
                    exact_approval,
                );
            }
        }
        let decision = crate::evaluation_result::denial(
            AuthorizationDenialReason::ApprovalConstraintConflict,
            effective_scope,
            observed,
            next_time_state,
        );
        proof {
            if exact_inputs {
                assert(approvals::effective_approval_conflict(
                    exact_approval,
                    decision.spec_scope_validity(),
                ));
                assert(approvals::decision_has_exact_approval(
                    policy,
                    &requested,
                    &decision,
                ));
            }
        }
        Ok(decision)
    }
}

} // verus!
