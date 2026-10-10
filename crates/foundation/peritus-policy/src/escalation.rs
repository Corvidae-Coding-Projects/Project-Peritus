//! Exact whole-request escalation challenges.

use crate::{ApprovalRequirement, AuthorityInstant, AuthorityTimeState, CapabilityScope, RiskSet};

#[doc = " Whole-request escalation challenge formed by conjunction of every matching requirement."]
#[derive(Debug, Eq, PartialEq)]
pub struct EscalationChallenge {
    scope: CapabilityScope,
    requirement: ApprovalRequirement,
    risks: RiskSet,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
}
impl EscalationChallenge {
    pub(crate) const fn new(
        scope: CapabilityScope,
        requirement: ApprovalRequirement,
        risks: RiskSet,
        evaluated_at: AuthorityInstant,
        time_state: AuthorityTimeState,
    ) -> Self {
        Self { scope, requirement, risks, evaluated_at, time_state }
    }
    #[doc = " Returns the complete effective scope challenged for approval."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
    #[doc = " Returns the conjunction of every applicable approval restriction."]
    #[must_use]
    pub const fn requirement(&self) -> &ApprovalRequirement {
        &self.requirement
    }
    #[doc = " Returns the exact authenticated operation-registry risk union."]
    #[must_use]
    pub const fn risks(&self) -> &RiskSet {
        &self.risks
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
    #[doc = " Consumes this challenge into its exact checked scope, requirement, evaluation instant,"]
    #[doc = " and monotonic successor time state."]
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (CapabilityScope, ApprovalRequirement, RiskSet, AuthorityInstant, AuthorityTimeState) {
        (self.scope, self.requirement, self.risks, self.evaluated_at, self.time_state)
    }
}
