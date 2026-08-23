//! Whole-request policy-denial value and exact scope views.

use crate::{
    AuthorityInstant, AuthorityTimeState, AuthorizationDenialReason, CapabilityScope,
};
use vstd::prelude::*;

verus! {

/// Whole-request denial with the unchanged requested scope and accepted time observation.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorizationDenial {
    reason: AuthorizationDenialReason,
    scope: CapabilityScope,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
}

impl AuthorizationDenial {
    /// Returns the exact denial reason used by specifications.
    pub closed spec fn spec_reason(&self) -> AuthorizationDenialReason { self.reason }

    /// Returns the exact denied scope's actor bytes used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.scope.spec_actor_id()
    }

    /// Returns the exact denied scope role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> crate::ActorRole { self.scope.spec_role() }

    /// Returns the exact denied scope environment bytes used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.scope.spec_environment_id()
    }

    /// Returns the exact denied permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<crate::Permission> {
        self.scope.spec_permissions()
    }

    /// Returns the exact denied revision used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> peritus_types::RevisionTuple {
        self.scope.spec_revision()
    }

    /// Returns the exact denied validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.scope.spec_validity()
    }

    /// Returns the exact denied use bound used by specifications.
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
        reason: AuthorizationDenialReason,
        scope: CapabilityScope,
        evaluated_at: AuthorityInstant,
        time_state: AuthorityTimeState,
    ) -> (denial: Self)
        ensures
            denial.spec_reason() == reason,
            denial.spec_scope_actor_id() == scope.spec_actor_id(),
            denial.spec_scope_role() == scope.spec_role(),
            denial.spec_scope_environment_id() == scope.spec_environment_id(),
            denial.spec_scope_permissions() == scope.spec_permissions(),
            denial.spec_scope_revision() == scope.spec_revision(),
            denial.spec_scope_validity() == scope.spec_validity(),
            denial.spec_scope_use_limit() == scope.spec_use_limit(),
            denial.spec_evaluated_at() == evaluated_at,
            denial.spec_time_epoch() == time_state.spec_epoch(),
            denial.spec_greatest_tick() == time_state.spec_greatest_tick_millis(),
    {
        Self { reason, scope, evaluated_at, time_state }
    }

    /// Returns the stable denial reason.
    #[must_use]
    pub const fn reason(&self) -> AuthorizationDenialReason { self.reason }

    /// Returns the complete request that was denied.
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope { &self.scope }

    /// Returns the accepted authority-time observation used by evaluation.
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant { self.evaluated_at }

    /// Returns the monotonic successor time state even though no authority was issued.
    #[must_use]
    pub const fn time_state(&self) -> &AuthorityTimeState { &self.time_state }
}

} // verus!
