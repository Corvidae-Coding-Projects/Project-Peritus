//! Move-only policy decision payloads and checked public accessors.

use crate::{
    AuthorityInstant, AuthorityTimeState, AuthorizationDenial, CapabilityScope,
    EscalationChallenge, PolicyDecision,
};
use vstd::prelude::*;

verus! {

/// Checked, effective scope ready for a logical capability-issuance transition.
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityIssuancePlan {
    scope: CapabilityScope,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
}

impl CapabilityIssuancePlan {
    /// Returns the exact authorized scope's actor bytes used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.scope.spec_actor_id()
    }

    /// Returns the exact authorized scope role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> crate::ActorRole { self.scope.spec_role() }

    /// Returns the exact authorized scope environment bytes used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.scope.spec_environment_id()
    }

    /// Returns the exact authorized permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<crate::Permission> {
        self.scope.spec_permissions()
    }

    /// Returns the exact authorized revision used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> peritus_types::RevisionTuple {
        self.scope.spec_revision()
    }

    /// Returns the exact effective validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.scope.spec_validity()
    }

    /// Returns the exact effective use bound used by specifications.
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

    /// This authority-bearing plan is deliberately move-only.
    ///
    /// ```compile_fail
    /// fn cannot_duplicate(plan: peritus_policy::CapabilityIssuancePlan) {
    ///     let _duplicate = plan.clone();
    /// }
    /// ```
    pub(crate) const fn new(
        scope: CapabilityScope,
        evaluated_at: AuthorityInstant,
        time_state: AuthorityTimeState,
    ) -> (plan: Self)
        ensures
            plan.spec_scope_actor_id() == scope.spec_actor_id(),
            plan.spec_scope_role() == scope.spec_role(),
            plan.spec_scope_environment_id() == scope.spec_environment_id(),
            plan.spec_scope_permissions() == scope.spec_permissions(),
            plan.spec_scope_revision() == scope.spec_revision(),
            plan.spec_scope_validity() == scope.spec_validity(),
            plan.spec_scope_use_limit() == scope.spec_use_limit(),
            plan.spec_evaluated_at() == evaluated_at,
            plan.spec_time_epoch() == time_state.spec_epoch(),
            plan.spec_greatest_tick() == time_state.spec_greatest_tick_millis(),
    {
        Self { scope, evaluated_at, time_state }
    }

    /// Returns the complete effective scope.
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope { &self.scope }

    /// Returns the accepted authority-time observation used by policy.
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant { self.evaluated_at }

    /// Returns the monotonic successor time state.
    #[must_use]
    pub const fn time_state(&self) -> &AuthorityTimeState { &self.time_state }

    pub(crate) fn into_parts(
        self,
    ) -> (parts: (CapabilityScope, AuthorityInstant, AuthorityTimeState))
        ensures
            parts.0.spec_actor_id() == self.spec_scope_actor_id(),
            parts.0.spec_role() == self.spec_scope_role(),
            parts.0.spec_environment_id() == self.spec_scope_environment_id(),
            parts.0.spec_permissions() == self.spec_scope_permissions(),
            parts.0.spec_revision() == self.spec_scope_revision(),
            parts.0.spec_validity() == self.spec_scope_validity(),
            parts.0.spec_use_limit() == self.spec_scope_use_limit(),
            parts.1 == self.spec_evaluated_at(),
            parts.2.spec_epoch() == self.spec_time_epoch(),
            parts.2.spec_greatest_tick_millis() == self.spec_greatest_tick(),
    {
        (self.scope, self.evaluated_at, self.time_state)
    }
}

/// Stable public tag for a total policy decision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyDecisionKind {
    /// The whole effective scope may proceed to logical capability issuance.
    Authorized,
    /// The whole effective scope requires one conjunction-satisfying approval.
    ApprovalRequired,
    /// The whole request was denied; no authorized subset exists.
    Denied,
}

impl PolicyDecision {
    /// Returns the exact decision tag used by specifications.
    pub closed spec fn spec_kind(&self) -> PolicyDecisionKind {
        match self {
            PolicyDecision::Authorized(_) => PolicyDecisionKind::Authorized,
            PolicyDecision::ApprovalRequired(_) => PolicyDecisionKind::ApprovalRequired,
            PolicyDecision::Denied(_) => PolicyDecisionKind::Denied,
        }
    }

    /// Connects the exhaustive public enum shape to its stable specification tag.
    pub(crate) proof fn variant_agrees_with_spec_kind(&self)
        ensures
            matches!(self, PolicyDecision::Authorized(_))
                == (self.spec_kind() == PolicyDecisionKind::Authorized),
            matches!(self, PolicyDecision::ApprovalRequired(_))
                == (self.spec_kind() == PolicyDecisionKind::ApprovalRequired),
            matches!(self, PolicyDecision::Denied(_))
                == (self.spec_kind() == PolicyDecisionKind::Denied),
    {
    }

    /// Returns the exact decided scope's actor bytes used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_actor_id(),
            PolicyDecision::ApprovalRequired(challenge) => {
                challenge.spec_scope_actor_id()
            }
            PolicyDecision::Denied(denial) => denial.spec_scope_actor_id(),
        }
    }

    /// Returns the exact decided scope role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> crate::ActorRole {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_role(),
            PolicyDecision::ApprovalRequired(challenge) => challenge.spec_scope_role(),
            PolicyDecision::Denied(denial) => denial.spec_scope_role(),
        }
    }

    /// Returns the exact decided scope's environment bytes used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_environment_id(),
            PolicyDecision::ApprovalRequired(challenge) => {
                challenge.spec_scope_environment_id()
            }
            PolicyDecision::Denied(denial) => denial.spec_scope_environment_id(),
        }
    }

    /// Returns the exact decided permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<crate::Permission> {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_permissions(),
            PolicyDecision::ApprovalRequired(challenge) => {
                challenge.spec_scope_permissions()
            }
            PolicyDecision::Denied(denial) => denial.spec_scope_permissions(),
        }
    }

    /// Returns the exact decided revision used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> peritus_types::RevisionTuple {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_revision(),
            PolicyDecision::ApprovalRequired(challenge) => challenge.spec_scope_revision(),
            PolicyDecision::Denied(denial) => denial.spec_scope_revision(),
        }
    }

    /// Returns the exact decided validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_validity(),
            PolicyDecision::ApprovalRequired(challenge) => {
                challenge.spec_scope_validity()
            }
            PolicyDecision::Denied(denial) => denial.spec_scope_validity(),
        }
    }

    /// Returns the exact decided use bound used by specifications.
    pub closed spec fn spec_scope_use_limit(&self) -> crate::UseLimit {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_scope_use_limit(),
            PolicyDecision::ApprovalRequired(challenge) => {
                challenge.spec_scope_use_limit()
            }
            PolicyDecision::Denied(denial) => denial.spec_scope_use_limit(),
        }
    }

    /// Returns the exact evaluation instant used by specifications.
    pub closed spec fn spec_evaluated_at(&self) -> AuthorityInstant {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_evaluated_at(),
            PolicyDecision::ApprovalRequired(challenge) => challenge.spec_evaluated_at(),
            PolicyDecision::Denied(denial) => denial.spec_evaluated_at(),
        }
    }

    /// Returns the accepted authority-time epoch used by specifications.
    pub closed spec fn spec_time_epoch(&self) -> int {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_time_epoch(),
            PolicyDecision::ApprovalRequired(challenge) => challenge.spec_time_epoch(),
            PolicyDecision::Denied(denial) => denial.spec_time_epoch(),
        }
    }

    /// Returns the accepted greatest authority tick used by specifications.
    pub closed spec fn spec_greatest_tick(&self) -> int {
        match self {
            PolicyDecision::Authorized(plan) => plan.spec_greatest_tick(),
            PolicyDecision::ApprovalRequired(challenge) => challenge.spec_greatest_tick(),
            PolicyDecision::Denied(denial) => denial.spec_greatest_tick(),
        }
    }

    /// Returns the denial reason, or `None` for non-denied decisions.
    pub closed spec fn spec_denial_reason(&self) -> Option<crate::AuthorizationDenialReason> {
        match self {
            PolicyDecision::Denied(denial) => Some(denial.spec_reason()),
            _ => None,
        }
    }

    pub(crate) const fn authorized(plan: CapabilityIssuancePlan) -> (decision: Self)
        ensures
            matches!(decision, PolicyDecision::Authorized(_)),
            decision == PolicyDecision::Authorized(plan),
            decision.spec_kind() == PolicyDecisionKind::Authorized,
            decision.spec_scope_actor_id() == plan.spec_scope_actor_id(),
            decision.spec_scope_role() == plan.spec_scope_role(),
            decision.spec_scope_environment_id() == plan.spec_scope_environment_id(),
            decision.spec_scope_permissions() == plan.spec_scope_permissions(),
            decision.spec_scope_revision() == plan.spec_scope_revision(),
            decision.spec_scope_validity() == plan.spec_scope_validity(),
            decision.spec_scope_use_limit() == plan.spec_scope_use_limit(),
            decision.spec_evaluated_at() == plan.spec_evaluated_at(),
            decision.spec_time_epoch() == plan.spec_time_epoch(),
            decision.spec_greatest_tick() == plan.spec_greatest_tick(),
    {
        Self::Authorized(plan)
    }

    pub(crate) const fn approval_required(challenge: EscalationChallenge) -> (decision: Self)
        ensures
            matches!(decision, PolicyDecision::ApprovalRequired(_)),
            decision == PolicyDecision::ApprovalRequired(challenge),
            decision.spec_kind() == PolicyDecisionKind::ApprovalRequired,
            decision.spec_scope_actor_id() == challenge.spec_scope_actor_id(),
            decision.spec_scope_role() == challenge.spec_scope_role(),
            decision.spec_scope_environment_id() == challenge.spec_scope_environment_id(),
            decision.spec_scope_permissions() == challenge.spec_scope_permissions(),
            decision.spec_scope_revision() == challenge.spec_scope_revision(),
            decision.spec_scope_validity() == challenge.spec_scope_validity(),
            decision.spec_scope_use_limit() == challenge.spec_scope_use_limit(),
            decision.spec_evaluated_at() == challenge.spec_evaluated_at(),
            decision.spec_time_epoch() == challenge.spec_time_epoch(),
            decision.spec_greatest_tick() == challenge.spec_greatest_tick(),
    {
        Self::ApprovalRequired(challenge)
    }

    pub(crate) const fn denied(denial: AuthorizationDenial) -> (decision: Self)
        ensures
            matches!(decision, PolicyDecision::Denied(_)),
            decision == PolicyDecision::Denied(denial),
            decision.spec_kind() == PolicyDecisionKind::Denied,
            decision.spec_scope_actor_id() == denial.spec_scope_actor_id(),
            decision.spec_scope_role() == denial.spec_scope_role(),
            decision.spec_scope_environment_id() == denial.spec_scope_environment_id(),
            decision.spec_scope_permissions() == denial.spec_scope_permissions(),
            decision.spec_scope_revision() == denial.spec_scope_revision(),
            decision.spec_scope_validity() == denial.spec_scope_validity(),
            decision.spec_scope_use_limit() == denial.spec_scope_use_limit(),
            decision.spec_evaluated_at() == denial.spec_evaluated_at(),
            decision.spec_time_epoch() == denial.spec_time_epoch(),
            decision.spec_greatest_tick() == denial.spec_greatest_tick(),
            decision.spec_denial_reason() == Some(denial.spec_reason()),
    {
        Self::Denied(denial)
    }

    /// Returns the stable decision tag without exposing its move-only payload.
    #[must_use]
    pub const fn kind(&self) -> PolicyDecisionKind {
        match self {
            Self::Authorized(_) => PolicyDecisionKind::Authorized,
            Self::ApprovalRequired(_) => PolicyDecisionKind::ApprovalRequired,
            Self::Denied(_) => PolicyDecisionKind::Denied,
        }
    }

    /// Borrows an authorized issuance plan, or returns `None` for another decision kind.
    #[must_use]
    pub const fn authorized_plan(&self) -> Option<&CapabilityIssuancePlan> {
        match self {
            Self::Authorized(plan) => Some(plan),
            _ => None,
        }
    }

    /// Borrows an escalation challenge, or returns `None` for another decision kind.
    #[must_use]
    pub const fn escalation_challenge(&self) -> Option<&EscalationChallenge> {
        match self {
            Self::ApprovalRequired(challenge) => Some(challenge),
            _ => None,
        }
    }

    /// Borrows a denial, or returns `None` for another decision kind.
    #[must_use]
    pub const fn denial(&self) -> Option<&AuthorizationDenial> {
        match self {
            Self::Denied(denial) => Some(denial),
            _ => None,
        }
    }

    /// Consumes the decision into an exact-one tuple of authorized, escalation, and denial values.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Option<CapabilityIssuancePlan>,
        Option<EscalationChallenge>,
        Option<AuthorizationDenial>,
    ) {
        match self {
            Self::Authorized(plan) => (Some(plan), None, None),
            Self::ApprovalRequired(challenge) => {
                (None, Some(challenge), None)
            }
            Self::Denied(denial) => (None, None, Some(denial)),
        }
    }
}

} // verus!
