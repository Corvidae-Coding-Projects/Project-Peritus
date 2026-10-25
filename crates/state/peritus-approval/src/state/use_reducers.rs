//! Exact single-use and amendment-use reducers.

use peritus_policy::{AuthorityInstant, PolicyRevisionCandidate};
use peritus_types::ActionId;

const fn use_failure(
    error: crate::ApprovalError,
    aggregate: super::ApprovalAggregate,
) -> crate::ApprovalUseFailure {
    crate::ApprovalUseFailure::new(error, aggregate)
}
#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl super::ApprovalAggregate {
    #[doc = " Consumes one exact approve-once decision at a monotonic valid observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the unchanged aggregate on phase, binding, time, or expiry failure."]
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    pub fn consume_once(
        self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
    ) -> Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure> {
        self.consume_once_checked(action_id, action_digest, observed_at)
    }
    #[allow(
        clippy::result_large_err,
        reason = "exact rejection owns the unchanged move-only aggregate"
    )]
    fn consume_once_checked(
        self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
    ) -> Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure> {
        let super::ApprovalState::ApprovedOnce(resolution) = self.state else {
            let error = if matches!(self.state, super::ApprovalState::Consumed(_)) {
                crate::ApprovalError::AlreadyConsumed
            } else {
                crate::ApprovalError::IllegalPhase {
                    expected: crate::ApprovalPhase::ApprovedOnce,
                    actual: self.phase(),
                }
            };
            return Err(use_failure(error, self));
        };
        if !super::exact::action_id_values_equal(action_id, self.request.action_id) {
            let error = crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Action);
            return Err(use_failure(error, self));
        }
        if !super::exact::action_digest_values_equal(action_digest, self.request.action_digest) {
            let error = crate::ApprovalError::BindingMismatch(crate::ScopeDimension::ActionDigest);
            return Err(use_failure(error, self));
        }
        if let Err(error) = self.request.validate_observation_time(observed_at) {
            return Err(use_failure(error, self));
        }
        if observed_at.epoch().get() != resolution.valid_until.epoch().get()
            || observed_at.tick_millis() >= resolution.valid_until.tick_millis()
        {
            let error = crate::ApprovalError::Expired;
            return Err(use_failure(error, self));
        }
        let revision = self.request.scope.revision();
        let Self { request: request, state: _ } = self;
        let request = super::advance_time_checked(request, observed_at);
        let transition = crate::ApprovedActionTransition::new(
            request.request_id,
            request.digest,
            action_id,
            action_digest,
            revision,
            resolution.decision_digest,
            resolution.command_id,
            resolution.registry_revision,
            resolution.valid_until,
        );
        let consumed =
            crate::ConsumedApproval::new(request.request_id, resolution.decision_digest, action_id);
        let outcome = crate::ApprovalUseOutcome {
            aggregate: Self { request, state: super::ApprovalState::Consumed(resolution) },
            transition,
            consumed,
        };
        Ok(outcome)
    }
    #[doc = " Consumes an amendment authorization only for its exact checked candidate."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the unchanged aggregate on phase, candidate-binding, time, or expiry failure."]
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    pub fn consume_amendment(
        self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
    ) -> Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure> {
        self.consume_amendment_checked(candidate, observed_at)
    }
    #[allow(
        clippy::result_large_err,
        reason = "exact rejection owns the unchanged move-only aggregate"
    )]
    fn consume_amendment_checked(
        self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
    ) -> Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure> {
        let super::ApprovalState::AmendmentAuthorized(resolution) = self.state else {
            return Err(super::transition_failure(
                crate::ApprovalError::IllegalPhase {
                    expected: crate::ApprovalPhase::AmendmentAuthorized,
                    actual: self.phase(),
                },
                self,
                None,
            ));
        };
        let crate::ApprovalChoice::Amend(identity) = resolution.choice else {
            return Err(super::transition_failure(crate::ApprovalError::CorruptState, self, None));
        };
        if !identity.matches_candidate(candidate) {
            return Err(super::transition_failure(
                crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Choice),
                self,
                None,
            ));
        }
        if let Err(error) = self.request.validate_observation_time(observed_at) {
            return Err(super::transition_failure(error, self, None));
        }
        if observed_at.epoch().get() != resolution.valid_until.epoch().get()
            || observed_at.tick_millis() >= resolution.valid_until.tick_millis()
        {
            let error = crate::ApprovalError::Expired;
            return Err(super::transition_failure(error, self, None));
        }
        let Self { request: request, state: _ } = self;
        let request = super::advance_time_checked(request, observed_at);
        let outcome = crate::ApprovalAmendmentOutcome {
            aggregate: Self { request, state: super::ApprovalState::Amended(resolution) },
            approval: crate::ApprovedPolicyAmendment::new(identity, resolution.registry_revision),
        };
        Ok(outcome)
    }
}
