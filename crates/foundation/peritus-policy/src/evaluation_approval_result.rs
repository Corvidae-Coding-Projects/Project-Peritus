//! Final decision construction when policy evaluation requires approval.

use crate::{
    ApprovalRequirement, AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState,
    AuthorizationDenialReason, CapabilityScope, EscalationChallenge, PolicyDecision,
    PolicyDefinition, UseLimit, ValidityWindow,
};

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling helper must remain inaccessible outside the crate"
)]
pub(super) fn finalize_required_approval(
    policy: &PolicyDefinition,
    requested: CapabilityScope,
    validity: ValidityWindow,
    use_limit: UseLimit,
    requirement: &ApprovalRequirement,
    time: AuthorityTimeState,
    observed: AuthorityInstant,
) -> Result<PolicyDecision, AuthorityTimeFailure> {
    let effective_scope = requested.with_constraints(validity, use_limit);
    let constrained = match requirement.constrain_validity(effective_scope.validity()) {
        Ok(result) => result,
        Err(error) => return Err(AuthorityTimeFailure::new(error, time)),
    };
    let next_time_state = time.observe(observed)?;
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
        Ok(decision)
    } else {
        let decision = crate::evaluation_result::denial(
            AuthorizationDenialReason::ApprovalConstraintConflict,
            effective_scope,
            observed,
            next_time_state,
        );
        Ok(decision)
    }
}
