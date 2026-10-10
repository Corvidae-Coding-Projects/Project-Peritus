//! Public whole-request policy-evaluation entry point.

use crate::{
    AuthorityInstant, AuthorityTimeFailure, AuthorityTimeState, AuthorizationRequest,
    PolicyDecision, PolicyDefinition, evaluation::evaluate_definition,
};

impl PolicyDefinition {
    #[doc = " Evaluates one exact whole request against the authenticated registry and immutable policy."]
    #[doc = ""]
    #[doc = " Semantic rejections have [`crate::PolicyDecisionKind::Denied`]."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed failure that owns the unchanged authority-time floor when time is"]
    #[doc = " inconsistent or exact constraint evaluation fails. No error result creates authority."]
    pub fn evaluate(
        &self,
        request: AuthorizationRequest,
        time_state: AuthorityTimeState,
        observed_at: AuthorityInstant,
    ) -> Result<PolicyDecision, AuthorityTimeFailure> {
        evaluate_definition(self, request, time_state, observed_at)
    }
}
