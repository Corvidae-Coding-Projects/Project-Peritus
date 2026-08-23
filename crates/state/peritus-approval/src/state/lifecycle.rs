//! Pending cancellation and exact expiry transitions.

use peritus_policy::AuthorityInstant;
use vstd::prelude::*;

verus! {

const fn pending_expiry(
    request: &crate::ApprovalRequest,
) -> (result: Result<AuthorityInstant, crate::ApprovalError>)
    ensures result == super::lifecycle_specification::pending_expiry_result(request),
{
    let request_expiry = request.validity.expires_at();
    let scope_validity = request.scope.validity();
    let scope_expiry = scope_validity.expires_at();
    let requirement_validity = request.requirement.validity();
    let requirement_expiry = requirement_validity.expires_at();
    assert(request_expiry == request.validity.spec_expires_at());
    assert(scope_validity == request.scope.spec_validity());
    assert(scope_expiry == request.scope.spec_validity().spec_expires_at());
    assert(requirement_validity == request.requirement.spec_validity());
    assert(requirement_expiry == request.requirement.spec_validity().spec_expires_at());
    let request_epoch = request_expiry.epoch().get();
    let scope_epoch = scope_expiry.epoch().get();
    let requirement_epoch = requirement_expiry.epoch().get();
    let request_tick = request_expiry.tick_millis();
    let scope_tick = scope_expiry.tick_millis();
    let requirement_tick = requirement_expiry.tick_millis();
    if request_epoch != scope_epoch || request_epoch != requirement_epoch
    {
        let result = Err(crate::ApprovalError::ClockEpochMismatch);
        assert(result == super::lifecycle_specification::pending_expiry_result(request));
        return result;
    }
    if request_tick <= scope_tick && request_tick <= requirement_tick
    {
        let result = Ok(request_expiry);
        assert(result == super::lifecycle_specification::pending_expiry_result(request));
        result
    } else if scope_tick <= requirement_tick {
        let result = Ok(scope_expiry);
        assert(result == super::lifecycle_specification::pending_expiry_result(request));
        result
    } else {
        let result = Ok(requirement_expiry);
        assert(result == super::lifecycle_specification::pending_expiry_result(request));
        result
    }
}

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands the move-only destructure to explicit field patterns"
)]
impl super::ApprovalAggregate {
    /// Expires a pending or unconsumed authorization at its exact earliest bound.
    ///
    /// # Errors
    ///
    /// Returns the unchanged aggregate before expiry, after a terminal phase, or on clock failure.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    #[allow(
        clippy::manual_map,
        clippy::option_if_let_else,
        reason = "explicit option matching is supported by pinned Verus and preserves proof visibility"
    )]
    pub fn expire(
        self,
        observed_at: AuthorityInstant,
    ) -> (result: Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>)
        ensures self.spec_expire_result_is_exact(observed_at, &result),
    {
        let ghost before = self;
        let result = self.expire_checked(observed_at);
        proof {
            super::lifecycle_specification::close_expire_relation(
                &before,
                observed_at,
                &result,
            );
        }
        result
    }

    #[allow(
        clippy::manual_map,
        clippy::option_if_let_else,
        clippy::result_large_err,
        reason = "explicit option projection preserves proof visibility and exact rejection owns the move-only aggregate"
    )]
    fn expire_checked(
        self,
        observed_at: AuthorityInstant,
    ) -> (result: Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>)
        ensures super::lifecycle_specification::expire_result_relation(
            &self,
            observed_at,
            &result,
        ),
    {
        let (expiry, resolution) = match self.state {
            super::ApprovalState::Pending => match pending_expiry(&self.request) {
                Ok(value) => (value, None),
                Err(error) => return Err(super::transition_failure(error, self, None)),
            },
            super::ApprovalState::ApprovedOnce(value)
            | super::ApprovalState::AmendmentAuthorized(value) => (value.valid_until, Some(value)),
            _ => {
                let error = crate::ApprovalError::IllegalPhase {
                    expected: crate::ApprovalPhase::Pending,
                    actual: self.phase(),
                };
                assert(self.spec_expire_expected_error(observed_at) == Some(error));
                return Err(super::transition_failure(error, self, None));
            }
        };
        if observed_at.epoch().get() != expiry.epoch().get() {
            let error = crate::ApprovalError::ClockEpochMismatch;
            assert(self.spec_expire_expected_error(observed_at) == Some(error));
            return Err(super::transition_failure(error, self, None));
        }
        if observed_at.tick_millis() < expiry.tick_millis() {
            let error = crate::ApprovalError::NotYetValid;
            assert(self.spec_expire_expected_error(observed_at) == Some(error));
            return Err(super::transition_failure(error, self, None));
        }
        if let Err(error) = self.request.validate_observation_time(observed_at) {
            assert(self.spec_expire_expected_error(observed_at) == Some(error));
            return Err(super::transition_failure(error, self, None));
        }
        proof { self.request.observation_time_ok(observed_at); }
        let from = self.phase();
        let Self { request, state: _ } = self;
        let request = super::advance_time_checked(request, observed_at);
        let outcome = super::ApprovalTransitionOutcome {
            aggregate: Self { request, state: super::ApprovalState::Expired(resolution) },
            transition: super::ApprovalTransition {
                kind: super::ApprovalTransitionKind::Expired,
                from,
                to: crate::ApprovalPhase::Expired,
                decision_digest: match resolution {
                    Some(value) => Some(value.decision_digest),
                    None => None,
                },
                registry_revision: match resolution {
                    Some(value) => Some(value.registry_revision),
                    None => None,
                },
            },
        };
        proof {
            outcome.prove_model();
            super::specification::expire_refines(
                &self,
                &outcome.aggregate,
                resolution,
            );
            crate::proofs::accepted_reducer_refines(
                self.spec_model(),
                crate::model::ApprovalModelStep::Expire,
                outcome.spec_model(),
            );
            assert(self.spec_expire_success_is_exact(observed_at, &outcome));
        }
        Ok(outcome)
    }
}

} // verus!
