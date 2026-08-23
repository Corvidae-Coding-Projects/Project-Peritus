//! Public whole-request policy-evaluation entry point.

use crate::{
    evaluation::evaluate_definition, AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState,
    AuthorizationRequest, PolicyDecision, PolicyDefinition,
};
use vstd::prelude::*;

verus! {

impl PolicyDefinition {
    /// Evaluates one exact whole request against the authenticated registry and immutable policy.
    ///
    /// Semantic rejections have [`crate::PolicyDecisionKind::Denied`].
    ///
    /// # Errors
    ///
    /// Returns a typed failure that owns the unchanged authority-time floor when time is
    /// inconsistent or exact constraint evaluation fails. No error result creates authority.
    pub fn evaluate(
        &self,
        request: AuthorizationRequest,
        time_state: AuthorityTimeState,
        observed_at: AuthorityInstant,
    ) -> (result: Result<PolicyDecision, AuthorityTimeFailure>)
        ensures
            crate::evaluation_outcome_model::evaluation_result_is_exact(
                self,
                &request,
                time_state,
                observed_at,
                &result,
            ),
            match result {
                Ok(decision) => {
                    crate::model::policy_evaluation_safety(
                        self,
                        &request,
                        time_state,
                        observed_at,
                        &decision,
                    )
                        && decision.spec_scope_actor_id() == request.spec_actor_id()
                        && decision.spec_scope_role() == request.spec_role()
                        && decision.spec_scope_environment_id()
                            == request.spec_environment_id()
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
        evaluate_definition(self, request, time_state, observed_at)
    }
}

} // verus!
