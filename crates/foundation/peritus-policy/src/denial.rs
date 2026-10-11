//! Whole-request policy-denial value and exact scope views.

use crate::{AuthorityInstant, AuthorityTimeState, AuthorizationDenialReason, CapabilityScope};

#[doc = " Whole-request denial with the unchanged requested scope and accepted time observation."]
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorizationDenial {
    reason: AuthorizationDenialReason,
    scope: CapabilityScope,
    evaluated_at: AuthorityInstant,
    time_state: AuthorityTimeState,
}
impl AuthorizationDenial {
    pub(crate) const fn new(
        reason: AuthorizationDenialReason,
        scope: CapabilityScope,
        evaluated_at: AuthorityInstant,
        time_state: AuthorityTimeState,
    ) -> Self {
        Self { reason, scope, evaluated_at, time_state }
    }
    #[doc = " Returns the stable denial reason."]
    #[must_use]
    pub const fn reason(&self) -> AuthorizationDenialReason {
        self.reason
    }
    #[doc = " Returns the complete request that was denied."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
    #[doc = " Returns the accepted authority-time observation used by evaluation."]
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant {
        self.evaluated_at
    }
    #[doc = " Returns the monotonic successor time state even though no authority was issued."]
    #[must_use]
    pub const fn time_state(&self) -> &AuthorityTimeState {
        &self.time_state
    }
}
