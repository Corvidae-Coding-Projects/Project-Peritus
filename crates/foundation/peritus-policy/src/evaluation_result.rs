//! Construction of final policy decisions after all policy folds succeed.

use crate::{
    ApprovalRequirement, AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState,
    AuthorizationDenial, AuthorizationDenialReason, AuthorizationRequest, CapabilityIssuancePlan,
    CapabilityScope, PolicyDecision, PolicyDefinition, UseLimit, ValidityWindow,
};

pub const fn denial(
    reason: AuthorizationDenialReason,
    scope: CapabilityScope,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
) -> PolicyDecision {
    PolicyDecision::denied(AuthorizationDenial::new(reason, scope, evaluated_at, time_state))
}
pub fn exact_requested_denial(
    _policy: &PolicyDefinition,
    request: AuthorizationRequest,
    previous_time: AuthorityTimeState,
    observed_at: AuthorityInstant,
    reason: AuthorizationDenialReason,
) -> Result<PolicyDecision, AuthorityTimeFailure> {
    let next_time_state = previous_time.observe(observed_at)?;
    let scope = request.into_scope();
    let decision = denial(reason, scope, observed_at, next_time_state);
    Ok(decision)
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling helper must remain inaccessible outside the crate"
)]
pub(super) fn finalize_decision(
    policy: &PolicyDefinition,
    requested: CapabilityScope,
    validity: ValidityWindow,
    use_limit: UseLimit,
    approval: Option<ApprovalRequirement>,
    time: AuthorityTimeState,
    observed: AuthorityInstant,
) -> Result<PolicyDecision, AuthorityTimeFailure> {
    let Some(requirement) = approval else {
        let effective_scope = requested.with_constraints(validity, use_limit);
        let next_time_state = time.observe(observed)?;
        let decision = PolicyDecision::authorized(CapabilityIssuancePlan::new(
            effective_scope,
            observed,
            next_time_state,
        ));
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
