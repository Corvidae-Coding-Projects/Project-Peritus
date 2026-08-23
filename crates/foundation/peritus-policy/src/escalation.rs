//! Exact whole-request escalation challenges.

use crate::{
    ApprovalRequirement, AuthorityInstant, AuthorityTimeState, CapabilityScope, RiskSet,
};
use vstd::prelude::*;

verus! {

/// Whole-request escalation challenge formed by conjunction of every matching requirement.
#[derive(Debug, Eq, PartialEq)]
pub struct EscalationChallenge {
    scope: CapabilityScope,
    requirement: ApprovalRequirement,
    risks: RiskSet,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
}

impl EscalationChallenge {
    /// Returns the exact policy-derived canonical mandatory-risk sequence.
    pub closed spec fn spec_risks(&self) -> Seq<crate::RiskClass> {
        self.risks.spec_values()
    }

    /// Returns the exact conjoined minimum approval tier.
    pub closed spec fn spec_requirement_minimum_tier(&self) -> crate::AuthorityTier {
        self.requirement.spec_minimum_tier()
    }

    /// Returns the exact conjoined approver-role sequence.
    pub closed spec fn spec_requirement_approver_roles(&self) -> Seq<crate::ActorRole> {
        self.requirement.spec_approver_roles()
    }

    /// Returns the exact conjoined independence sequence.
    pub closed spec fn spec_requirement_independence(
        &self,
    ) -> Seq<crate::IndependenceRequirement> {
        self.requirement.spec_independence()
    }

    /// Returns the exact conjoined approval validity.
    pub closed spec fn spec_requirement_validity(&self) -> crate::ValidityWindow {
        self.requirement.spec_validity()
    }
    /// Returns the exact challenged scope's actor bytes used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.scope.spec_actor_id()
    }

    /// Returns the exact challenged scope role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> crate::ActorRole { self.scope.spec_role() }

    /// Returns the exact challenged scope environment bytes used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.scope.spec_environment_id()
    }

    /// Returns the exact challenged permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<crate::Permission> {
        self.scope.spec_permissions()
    }

    /// Returns the exact challenged revision used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> peritus_types::RevisionTuple {
        self.scope.spec_revision()
    }

    /// Returns the exact challenged validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.scope.spec_validity()
    }

    /// Returns the exact challenged use bound used by specifications.
    pub closed spec fn spec_scope_use_limit(&self) -> crate::UseLimit {
        self.scope.spec_use_limit()
    }

    /// Returns the exact evaluation instant used by specifications.
    pub closed spec fn spec_evaluated_at(&self) -> AuthorityInstant { self.evaluated_at }

    /// Returns the accepted authority-time epoch used by specifications.
    pub closed spec fn spec_time_epoch(&self) -> int { self.time_state.spec_epoch() }

    /// Returns the accepted greatest authority tick used by specifications.
    pub closed spec fn spec_greatest_tick(&self) -> int {
        self.time_state.spec_greatest_tick_millis()
    }

    pub(crate) const fn new(
        scope: CapabilityScope,
        requirement: ApprovalRequirement,
        risks: RiskSet,
        evaluated_at: AuthorityInstant,
        time_state: AuthorityTimeState,
    ) -> (challenge: Self)
        ensures
            challenge.spec_scope_actor_id() == scope.spec_actor_id(),
            challenge.spec_scope_role() == scope.spec_role(),
            challenge.spec_scope_environment_id() == scope.spec_environment_id(),
            challenge.spec_scope_permissions() == scope.spec_permissions(),
            challenge.spec_scope_revision() == scope.spec_revision(),
            challenge.spec_scope_validity() == scope.spec_validity(),
            challenge.spec_scope_use_limit() == scope.spec_use_limit(),
            challenge.spec_requirement_minimum_tier()
                == requirement.spec_minimum_tier(),
            challenge.spec_requirement_approver_roles()
                == requirement.spec_approver_roles(),
            challenge.spec_requirement_independence()
                == requirement.spec_independence(),
            challenge.spec_requirement_validity() == requirement.spec_validity(),
            challenge.spec_risks() == risks.spec_values(),
            challenge.spec_evaluated_at() == evaluated_at,
            challenge.spec_time_epoch() == time_state.spec_epoch(),
            challenge.spec_greatest_tick() == time_state.spec_greatest_tick_millis(),
    {
        Self { scope, requirement, risks, evaluated_at, time_state }
    }

    /// Returns the complete effective scope challenged for approval.
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope { &self.scope }

    /// Returns the conjunction of every applicable approval restriction.
    #[must_use]
    pub const fn requirement(&self) -> &ApprovalRequirement { &self.requirement }

    /// Returns the exact authenticated operation-registry risk union.
    #[must_use]
    pub const fn risks(&self) -> &RiskSet { &self.risks }

    /// Returns the accepted authority-time observation used by policy.
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant { self.evaluated_at }

    /// Returns the monotonic successor time state.
    #[must_use]
    pub const fn time_state(&self) -> &AuthorityTimeState { &self.time_state }

    /// Consumes this challenge into its exact checked scope, requirement, evaluation instant,
    /// and monotonic successor time state.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (parts: (CapabilityScope, ApprovalRequirement, RiskSet, AuthorityInstant, AuthorityTimeState))
        ensures
            parts.0.spec_actor_id() == self.spec_scope_actor_id(),
            parts.0.spec_role() == self.spec_scope_role(),
            parts.0.spec_environment_id() == self.spec_scope_environment_id(),
            parts.0.spec_permissions() == self.spec_scope_permissions(),
            parts.0.spec_revision() == self.spec_scope_revision(),
            parts.0.spec_validity() == self.spec_scope_validity(),
            parts.0.spec_use_limit() == self.spec_scope_use_limit(),
            parts.1.spec_minimum_tier() == self.spec_requirement_minimum_tier(),
            parts.1.spec_approver_roles() == self.spec_requirement_approver_roles(),
            parts.1.spec_independence() == self.spec_requirement_independence(),
            parts.1.spec_validity() == self.spec_requirement_validity(),
            parts.2.spec_values() == self.spec_risks(),
            parts.3 == self.spec_evaluated_at(),
            parts.4.spec_epoch() == self.spec_time_epoch(),
            parts.4.spec_greatest_tick_millis() == self.spec_greatest_tick(),
    {
        (self.scope, self.requirement, self.risks, self.evaluated_at, self.time_state)
    }
}

} // verus!
