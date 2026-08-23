//! Move-only logical capability issuance and exact single-action use transitions.

use crate::{
    AuthorityInstant, AuthorityTimeState, CapabilityScope, CapabilityUseFailure,
    CapabilityUseRequest, CapabilityUseTransition, PolicyError, UseLimit,
};
#[cfg(verus_only)]
use crate::{ActorRole, Permission};
use peritus_types::{CommandId, Sha256Digest};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Move-only logical capability. It does not prove a transition was durably committed.
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
    /// Returns whether one use result exactly refines every accepted or rejected outcome.
    pub closed spec fn spec_try_use_result(
        &self,
        request: &CapabilityUseRequest,
        transition_digest: Sha256Digest,
        result: &Result<CapabilityUseTransition, CapabilityUseFailure>,
    ) -> bool {
        crate::capability_use_model::result_is_exact(
            self,
            request,
            transition_digest,
            result,
        )
    }

    /// Returns the exact issuance instant used by specifications.
    pub closed spec fn spec_issued_at(&self) -> AuthorityInstant { self.issued_at }

    /// Returns the exact issuance digest bytes used by specifications.
    pub closed spec fn spec_issuance_digest(&self) -> [u8; 32] {
        self.issuance_digest.spec_bytes()
    }

    /// Returns the exact issuance command identifier bytes used by specifications.
    pub closed spec fn spec_issuance_command_id(&self) -> [u8; 16] {
        self.issuance_command_id.spec_bytes()
    }

    pub(crate) const fn from_issuance(
        scope: CapabilityScope,
        issued_at: AuthorityInstant,
        issuance_digest: Sha256Digest,
        issuance_command_id: CommandId,
        time_state: AuthorityTimeState,
    ) -> (capability: Self)
        ensures
            capability.spec_scope_actor_id() == scope.spec_actor_id(),
            capability.spec_scope_role() == scope.spec_role(),
            capability.spec_scope_environment_id() == scope.spec_environment_id(),
            capability.spec_scope_permissions() == scope.spec_permissions(),
            capability.spec_scope_revision() == scope.spec_revision(),
            capability.spec_scope_validity() == scope.spec_validity(),
            capability.spec_remaining_uses() == scope.spec_use_limit().spec_remaining(),
            capability.spec_issued_at() == issued_at,
            capability.spec_issuance_digest() == issuance_digest.spec_bytes(),
            capability.spec_issuance_command_id() == issuance_command_id.spec_bytes(),
            capability.spec_time_epoch() == time_state.spec_epoch(),
            capability.spec_greatest_tick() == time_state.spec_greatest_tick_millis(),
    {
        let remaining_uses = scope.use_limit();
        Self {
            scope,
            issued_at,
            issuance_digest,
            issuance_command_id,
            time_state,
            remaining_uses,
        }
    }
    fn validate_use_time(
        &self,
        observed_at: AuthorityInstant,
    ) -> (result: Result<(), PolicyError>)
        ensures
            match result {
                Ok(()) => {
                    crate::capability_use_model::time_error(self, observed_at).is_none()
                        && self.time_state.spec_accepts(observed_at)
                        && observed_at.spec_epoch() == self.spec_time_epoch()
                        && observed_at.spec_tick_millis() >= self.spec_greatest_tick()
                        && self.spec_scope_validity().spec_contains(observed_at)
                }
                Err(error) => {
                    crate::capability_use_model::time_error(self, observed_at)
                        == Some(error.spec_kind())
                        && error.spec_dimension().is_none()
                        && error.spec_collection().is_none()
                }
            },
    {
        match self.time_state.validate_observation(observed_at) {
            Ok(()) => {}
            Err(error) => return Err(error),
        }
        assert(self.time_state.spec_accepts(observed_at));
        assert(observed_at.spec_epoch() == self.spec_time_epoch());
        assert(observed_at.spec_tick_millis() >= self.spec_greatest_tick());
        let validity = self.scope.validity();
        let is_current = validity.contains(observed_at)?;
        if !is_current && observed_at.tick_millis() < validity.not_before().tick_millis() {
            Err(PolicyError::capability_not_yet_valid())
        } else if !is_current {
            Err(PolicyError::capability_expired())
        } else {
            assert(validity.spec_contains(observed_at));
            assert(self.spec_scope_validity().spec_contains(observed_at));
            Ok(())
        }
    }

    /// Returns the exact actor identity bytes used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.scope.spec_actor_id()
    }

    /// Returns the exact stable role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> ActorRole { self.scope.spec_role() }

    /// Returns the exact environment identity bytes used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.scope.spec_environment_id()
    }

    /// Returns the exact ordered permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<Permission> {
        self.scope.spec_permissions()
    }

    /// Returns exact comparator-based scope membership used by specifications.
    pub closed spec fn spec_scope_contains_permission(&self, permission: &Permission) -> bool {
        self.scope.spec_contains_permission(permission)
    }

    /// Returns the exact revision tuple used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> RevisionTuple {
        self.scope.spec_revision()
    }

    /// Returns the exact validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.scope.spec_validity()
    }

    /// Returns the exact immutable use limit carried by the capability scope.
    pub closed spec fn spec_scope_use_limit(&self) -> Option<int> {
        self.scope.spec_use_limit().spec_remaining()
    }

    /// Returns the exact remaining logical-use bound used by specifications.
    pub closed spec fn spec_remaining_uses(&self) -> Option<int> {
        self.remaining_uses.spec_remaining()
    }

    /// Returns the exact capability time-state epoch used by specifications.
    pub closed spec fn spec_time_epoch(&self) -> int { self.time_state.spec_epoch() }

    /// Returns the greatest accepted capability tick used by specifications.
    pub closed spec fn spec_greatest_tick(&self) -> int {
        self.time_state.spec_greatest_tick_millis()
    }

    /// Returns the complete effective issuance scope.
    #[must_use]
    pub const fn scope(&self) -> (scope: &CapabilityScope)
        ensures
            scope.spec_actor_id() == self.spec_scope_actor_id(),
            scope.spec_role() == self.spec_scope_role(),
            scope.spec_environment_id() == self.spec_scope_environment_id(),
            scope.spec_permissions() == self.spec_scope_permissions(),
            scope.spec_revision() == self.spec_scope_revision(),
            scope.spec_validity() == self.spec_scope_validity(),
            scope.spec_use_limit().spec_remaining() == self.spec_scope_use_limit(),
    { &self.scope }

    /// Returns the authority instant at which policy authorized issuance.
    #[must_use]
    pub const fn issued_at(&self) -> (issued_at: AuthorityInstant)
        ensures issued_at == self.spec_issued_at(),
    { self.issued_at }

    /// Returns the digest that binds this capability to its issuance transition.
    #[must_use]
    pub const fn issuance_digest(&self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_issuance_digest(),
    { self.issuance_digest }

    /// Returns the unique command identity bound to logical issuance and durable replay.
    #[must_use]
    pub const fn issuance_command_id(&self) -> (command_id: CommandId)
        ensures command_id.spec_bytes() == self.spec_issuance_command_id(),
    { self.issuance_command_id }

    /// Returns the greatest authority-time observation accepted by this capability.
    #[must_use]
    pub const fn time_state(&self) -> (state: &AuthorityTimeState)
        ensures
            state.spec_epoch() == self.spec_time_epoch(),
            state.spec_greatest_tick_millis() == self.spec_greatest_tick(),
    { &self.time_state }

    /// Returns the exact remaining logical-use bound.
    #[must_use]
    pub const fn remaining_uses(&self) -> (remaining: UseLimit)
        ensures remaining.spec_remaining() == self.spec_remaining_uses(),
    { self.remaining_uses }

    /// Attempts one exact, action-bound logical use and returns the unchanged capability on failure.
    ///
    /// # Errors
    ///
    /// The error wrapper reports the exact scope, time, expiry, or exhaustion failure and owns the
    /// unchanged prior capability. A successful transition decrements a limited use exactly once.
    // The deliberately large error preserves linear ownership of the unchanged capability; boxing
    // would move this verified authority boundary behind an unsupported allocation abstraction.
    #[allow(clippy::result_large_err)]
    pub fn try_use(
        self,
        request: CapabilityUseRequest,
        transition_digest: Sha256Digest,
    ) -> (result: Result<CapabilityUseTransition, CapabilityUseFailure>)
        ensures self.spec_try_use_result(&request, transition_digest, &result),
    {
        if let Some(dimension) = request.scope_mismatch(&self.scope) {
            return Err(CapabilityUseFailure::new(
                PolicyError::capability_scope_mismatch(dimension),
                self,
            ));
        }
        assert(crate::model::same_identifier(
            request.spec_actor_id(),
            self.spec_scope_actor_id()
        ));
        assert(request.spec_role() == self.spec_scope_role());
        assert(crate::model::same_identifier(
            request.spec_environment_id(),
            self.spec_scope_environment_id()
        ));
        assert(crate::model::same_revision(
            request.spec_revision(),
            self.spec_scope_revision()
        ));
        assert(self.spec_scope_contains_permission(&request.spec_permission()));

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
        assert(successor.spec_scope_actor_id() == self.spec_scope_actor_id());
        assert(successor.spec_scope_role() == self.spec_scope_role());
        assert(successor.spec_scope_environment_id() == self.spec_scope_environment_id());
        assert(successor.spec_scope_permissions() == self.spec_scope_permissions());
        assert(successor.spec_scope_revision() == self.spec_scope_revision());
        assert(successor.spec_scope_validity() == self.spec_scope_validity());
        assert(successor.spec_time_epoch() == request.spec_observed_at().spec_epoch());
        assert(
            successor.spec_greatest_tick()
                == request.spec_observed_at().spec_tick_millis()
        );
        assert(crate::model::use_limit_successor(
            self.spec_remaining_uses(),
            successor.spec_remaining_uses()
        ));
        let transition = CapabilityUseTransition::new(
            request,
            transition_digest,
            previous_remaining,
            successor,
        );
        Ok(transition)
    }
}

} // verus!
