//! Move-only policy decision payloads and checked public accessors.

use crate::{
    AuthorityInstant, AuthorityTimeState, AuthorizationDenial, CapabilityScope,
    EscalationChallenge, PolicyDecision,
};

#[doc = " Checked, effective scope ready for a logical capability-issuance transition."]
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityIssuancePlan {
    scope: CapabilityScope,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
}
impl CapabilityIssuancePlan {
    #[doc = " This authority-bearing plan is deliberately move-only."]
    #[doc = ""]
    #[doc = " ```compile_fail"]
    #[doc = " fn cannot_duplicate(plan: peritus_policy::CapabilityIssuancePlan) {"]
    #[doc = "     let _duplicate = plan.clone();"]
    #[doc = " }"]
    #[doc = " ```"]
    pub(crate) const fn new(
        scope: CapabilityScope,
        evaluated_at: AuthorityInstant,
        time_state: AuthorityTimeState,
    ) -> Self {
        Self { scope, evaluated_at, time_state }
    }
    #[doc = " Returns the complete effective scope."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
    #[doc = " Returns the accepted authority-time observation used by policy."]
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant {
        self.evaluated_at
    }
    #[doc = " Returns the monotonic successor time state."]
    #[must_use]
    pub const fn time_state(&self) -> &AuthorityTimeState {
        &self.time_state
    }
    pub(crate) fn into_parts(self) -> (CapabilityScope, AuthorityInstant, AuthorityTimeState) {
        (self.scope, self.evaluated_at, self.time_state)
    }
}
#[doc = " Stable public tag for a total policy decision."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyDecisionKind {
    #[doc = " The whole effective scope may proceed to logical capability issuance."]
    Authorized,
    #[doc = " The whole effective scope requires one conjunction-satisfying approval."]
    ApprovalRequired,
    #[doc = " The whole request was denied; no authorized subset exists."]
    Denied,
}
impl PolicyDecision {
    pub(crate) const fn authorized(plan: CapabilityIssuancePlan) -> Self {
        Self::Authorized(plan)
    }
    pub(crate) const fn approval_required(challenge: EscalationChallenge) -> Self {
        Self::ApprovalRequired(challenge)
    }
    pub(crate) const fn denied(denial: AuthorizationDenial) -> Self {
        Self::Denied(denial)
    }
    #[doc = " Returns the stable decision tag without exposing its move-only payload."]
    #[must_use]
    pub const fn kind(&self) -> PolicyDecisionKind {
        match self {
            Self::Authorized(_) => PolicyDecisionKind::Authorized,
            Self::ApprovalRequired(_) => PolicyDecisionKind::ApprovalRequired,
            Self::Denied(_) => PolicyDecisionKind::Denied,
        }
    }
    #[doc = " Borrows an authorized issuance plan, or returns `None` for another decision kind."]
    #[must_use]
    pub const fn authorized_plan(&self) -> Option<&CapabilityIssuancePlan> {
        match self {
            Self::Authorized(plan) => Some(plan),
            _ => None,
        }
    }
    #[doc = " Borrows an escalation challenge, or returns `None` for another decision kind."]
    #[must_use]
    pub const fn escalation_challenge(&self) -> Option<&EscalationChallenge> {
        match self {
            Self::ApprovalRequired(challenge) => Some(challenge),
            _ => None,
        }
    }
    #[doc = " Borrows a denial, or returns `None` for another decision kind."]
    #[must_use]
    pub const fn denial(&self) -> Option<&AuthorizationDenial> {
        match self {
            Self::Denied(denial) => Some(denial),
            _ => None,
        }
    }
    #[doc = " Consumes the decision into an exact-one tuple of authorized, escalation, and denial values."]
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (Option<CapabilityIssuancePlan>, Option<EscalationChallenge>, Option<AuthorizationDenial>)
    {
        match self {
            Self::Authorized(plan) => (Some(plan), None, None),
            Self::ApprovalRequired(challenge) => (None, Some(challenge), None),
            Self::Denied(denial) => (None, None, Some(denial)),
        }
    }
}
