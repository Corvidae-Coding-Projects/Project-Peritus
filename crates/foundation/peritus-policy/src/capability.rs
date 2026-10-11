//! Move-only logical capability issuance and exact single-action use transitions.

use crate::{
    AuthorityInstant, AuthorityTimeState, CapabilityScope, CapabilityUseFailure,
    CapabilityUseRequest, CapabilityUseTransition, PolicyError, UseLimit,
};

use peritus_types::{CommandId, Sha256Digest};

#[doc = " Move-only logical capability. It does not prove a transition was durably committed."]
#[derive(Debug, Eq, PartialEq)]
pub struct Capability {
    scope: CapabilityScope,
    issued_at: AuthorityInstant,
    issuance_digest: Sha256Digest,
    issuance_command_id: CommandId,
    time_state: AuthorityTimeState,
    remaining_uses: UseLimit,
}
impl Capability {
    pub(crate) const fn from_issuance(
        scope: CapabilityScope,
        issued_at: AuthorityInstant,
        issuance_digest: Sha256Digest,
        issuance_command_id: CommandId,
        time_state: AuthorityTimeState,
    ) -> Self {
        let remaining_uses = scope.use_limit();
        Self { scope, issued_at, issuance_digest, issuance_command_id, time_state, remaining_uses }
    }
    fn validate_use_time(&self, observed_at: AuthorityInstant) -> Result<(), PolicyError> {
        match self.time_state.validate_observation(observed_at) {
            Ok(()) => {}
            Err(error) => return Err(error),
        }
        let validity = self.scope.validity();
        let is_current = validity.contains(observed_at)?;
        if !is_current && observed_at.tick_millis() < validity.not_before().tick_millis() {
            Err(PolicyError::capability_not_yet_valid())
        } else if !is_current {
            Err(PolicyError::capability_expired())
        } else {
            Ok(())
        }
    }
    #[doc = " Returns the complete effective issuance scope."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
    #[doc = " Returns the authority instant at which policy authorized issuance."]
    #[must_use]
    pub const fn issued_at(&self) -> AuthorityInstant {
        self.issued_at
    }
    #[doc = " Returns the digest that binds this capability to its issuance transition."]
    #[must_use]
    pub const fn issuance_digest(&self) -> Sha256Digest {
        self.issuance_digest
    }
    #[doc = " Returns the unique command identity bound to logical issuance and durable replay."]
    #[must_use]
    pub const fn issuance_command_id(&self) -> CommandId {
        self.issuance_command_id
    }
    #[doc = " Returns the greatest authority-time observation accepted by this capability."]
    #[must_use]
    pub const fn time_state(&self) -> &AuthorityTimeState {
        &self.time_state
    }
    #[doc = " Returns the exact remaining logical-use bound."]
    #[must_use]
    pub const fn remaining_uses(&self) -> UseLimit {
        self.remaining_uses
    }
    #[doc = " Attempts one exact, action-bound logical use and returns the unchanged capability on failure."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " The error wrapper reports the exact scope, time, expiry, or exhaustion failure and owns the"]
    #[doc = " unchanged prior capability. A successful transition decrements a limited use exactly once."]
    #[allow(clippy::result_large_err)]
    pub fn try_use(
        self,
        request: CapabilityUseRequest,
        transition_digest: Sha256Digest,
    ) -> Result<CapabilityUseTransition, CapabilityUseFailure> {
        if let Some(dimension) = request.scope_mismatch(&self.scope) {
            return Err(CapabilityUseFailure::new(
                PolicyError::capability_scope_mismatch(dimension),
                self,
            ));
        }
        let observed_at = request.observed_at();
        match self.validate_use_time(observed_at) {
            Ok(()) => {}
            Err(error) => return Err(CapabilityUseFailure::new(error, self)),
        }
        let previous_remaining = self.remaining_uses;
        let remaining_uses = match previous_remaining.decrement() {
            Ok(value) => value,
            Err(error) => return Err(CapabilityUseFailure::new(error, self)),
        };
        let scope = self.scope;
        let issued_at = self.issued_at;
        let issuance_digest = self.issuance_digest;
        let issuance_command_id = self.issuance_command_id;
        let time_state = self.time_state;
        let next_time_state = match time_state.observe(observed_at) {
            Ok(next) => next,
            Err(failure) => {
                let error = failure.error();
                let restored = Self {
                    scope,
                    issued_at,
                    issuance_digest,
                    issuance_command_id,
                    time_state: failure.into_state(),
                    remaining_uses: previous_remaining,
                };
                return Err(CapabilityUseFailure::new(error, restored));
            }
        };
        let successor = Self {
            scope,
            issued_at,
            issuance_digest,
            issuance_command_id,
            time_state: next_time_state,
            remaining_uses,
        };
        let transition =
            CapabilityUseTransition::new(request, transition_digest, previous_remaining, successor);
        Ok(transition)
    }
}
