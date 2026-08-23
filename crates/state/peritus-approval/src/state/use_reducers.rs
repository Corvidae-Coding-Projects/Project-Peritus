//! Exact single-use and amendment-use reducers.

use peritus_policy::{AuthorityInstant, PolicyRevisionCandidate};
use peritus_types::ActionId;
use vstd::prelude::*;

verus! {

const fn use_failure(
    error: crate::ApprovalError,
    aggregate: super::ApprovalAggregate,
) -> (failure: crate::ApprovalUseFailure)
    ensures
        failure.error == error,
        failure.aggregate == aggregate,
        failure.spec_preserves_aggregate(&aggregate),
{
    let failure = crate::ApprovalUseFailure::new(error, aggregate);
    proof {
        failure.prove_preserves(&aggregate);
        crate::proofs::rejected_reducer_preserves(&aggregate, &failure.aggregate);
    }
    failure
}

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl super::ApprovalAggregate {
    /// Consumes one exact approve-once decision at a monotonic valid observation.
    ///
    /// # Errors
    ///
    /// Returns the unchanged aggregate on phase, binding, time, or expiry failure.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    pub fn consume_once(
        self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
    ) -> (result: Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure>)
        ensures self.spec_consume_once_result_is_exact(
            action_id,
            action_digest,
            observed_at,
            &result,
        ),
    {
        let ghost before = self;
        let result = self.consume_once_checked(action_id, action_digest, observed_at);
        proof {
            super::use_specification::close_consume_once_relation(
                &before,
                action_id,
                action_digest,
                observed_at,
                &result,
            );
        }
        result
    }

    fn consume_once_checked(
        self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
    ) -> (result: Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure>)
        ensures super::use_specification::consume_once_result_relation(
            &self,
            action_id,
            action_digest,
            observed_at,
            &result,
        ),
    {
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
        assert(self.state == super::ApprovalState::ApprovedOnce(resolution));
        if !super::exact::action_id_values_equal(action_id, self.request.action_id) {
            let error = crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Action);
            assert(self.spec_consume_once_expected_error(
                action_id, action_digest, observed_at,
            ) == Some(error));
            return Err(use_failure(error, self));
        }
        if !super::exact::action_digest_values_equal(action_digest, self.request.action_digest) {
            let error = crate::ApprovalError::BindingMismatch(crate::ScopeDimension::ActionDigest);
            assert(self.spec_consume_once_expected_error(
                action_id, action_digest, observed_at,
            ) == Some(error));
            return Err(use_failure(error, self));
        }
        if let Err(error) = self.request.validate_observation_time(observed_at) {
            assert(self.spec_consume_once_expected_error(
                action_id, action_digest, observed_at,
            ) == Some(error));
            return Err(use_failure(error, self));
        }
        proof { self.request.observation_time_ok(observed_at); }
        if observed_at.epoch().get() != resolution.valid_until.epoch().get()
            || observed_at.tick_millis() >= resolution.valid_until.tick_millis()
        {
            let error = crate::ApprovalError::Expired;
            assert(self.spec_consume_once_expected_error(
                action_id, action_digest, observed_at,
            ) == Some(error));
            return Err(use_failure(error, self));
        }
        let revision = self.request.scope.revision();
        assert(revision == self.request.scope.spec_revision());
        let Self { request, state: _ } = self;
        let request = super::advance_time_checked(request, observed_at);
        let transition = crate::ApprovedActionTransition::new(
            request.request_id, request.digest, action_id, action_digest,
            revision, resolution.decision_digest, resolution.command_id,
            resolution.registry_revision, resolution.valid_until,
        );
        let consumed = crate::ConsumedApproval::new(
            request.request_id, resolution.decision_digest, action_id,
        );
        let outcome = crate::ApprovalUseOutcome {
            aggregate: Self { request, state: super::ApprovalState::Consumed(resolution) },
            transition,
            consumed,
        };
        proof {
            outcome.prove_model();
            super::specification::consume_once_refines(&self, &outcome.aggregate, resolution);
            crate::proofs::consume_once_preserves_digest(self.spec_model());
            crate::proofs::accepted_reducer_refines(
                self.spec_model(),
                crate::model::ApprovalModelStep::ConsumeOnce,
                outcome.spec_model(),
            );
            assert(super::exact::request_is_exact_advance(
                &outcome.aggregate.request,
                &self.request,
                observed_at,
            ));
            assert(outcome.aggregate.state == super::ApprovalState::Consumed(resolution));
            assert(outcome.transition.request_id == self.request.request_id);
            assert(outcome.transition.request_digest == self.request.digest);
            assert(outcome.transition.action_id == action_id);
            assert(outcome.transition.action_digest == action_digest);
            assert(outcome.transition.revision == self.request.scope.spec_revision());
            assert(outcome.transition.decision_digest == resolution.decision_digest);
            assert(outcome.transition.command_id == resolution.command_id);
            assert(outcome.transition.registry_revision == resolution.registry_revision);
            assert(outcome.transition.valid_until == resolution.valid_until);
            assert(outcome.consumed.request_id == self.request.request_id);
            assert(outcome.consumed.decision_digest == resolution.decision_digest);
            assert(outcome.consumed.action_id == action_id);
            assert(self.spec_consume_once_success_is_exact(
                action_id, action_digest, observed_at, &outcome,
            ));
        }
        Ok(outcome)
    }

    /// Consumes an amendment authorization only for its exact checked candidate.
    ///
    /// # Errors
    ///
    /// Returns the unchanged aggregate on phase, candidate-binding, time, or expiry failure.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must own the unchanged move-only aggregate"
    )]
    pub fn consume_amendment(
        self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
    ) -> (result: Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure>)
        ensures self.spec_consume_amendment_result_is_exact(
            candidate,
            observed_at,
            &result,
        ),
    {
        let ghost before = self;
        let result = self.consume_amendment_checked(candidate, observed_at);
        proof {
            super::use_specification::close_consume_amendment_relation(
                &before,
                candidate,
                observed_at,
                &result,
            );
        }
        result
    }

    fn consume_amendment_checked(
        self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
    ) -> (result: Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure>)
        ensures super::use_specification::consume_amendment_result_relation(
            &self,
            candidate,
            observed_at,
            &result,
        ),
    {
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
        assert(self.state == super::ApprovalState::AmendmentAuthorized(resolution));
        let crate::ApprovalChoice::Amend(identity) = resolution.choice else {
            return Err(super::transition_failure(crate::ApprovalError::CorruptState, self, None));
        };
        if !identity.matches_candidate(candidate) {
            return Err(super::transition_failure(
                crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Choice), self, None,
            ));
        }
        if let Err(error) = self.request.validate_observation_time(observed_at) {
            assert(self.spec_consume_amendment_expected_error(candidate, observed_at)
                == Some(error));
            return Err(super::transition_failure(error, self, None));
        }
        proof { self.request.observation_time_ok(observed_at); }
        if observed_at.epoch().get() != resolution.valid_until.epoch().get()
            || observed_at.tick_millis() >= resolution.valid_until.tick_millis()
        {
            let error = crate::ApprovalError::Expired;
            assert(self.spec_consume_amendment_expected_error(candidate, observed_at)
                == Some(error));
            return Err(super::transition_failure(error, self, None));
        }
        let Self { request, state: _ } = self;
        let request = super::advance_time_checked(request, observed_at);
        let outcome = crate::ApprovalAmendmentOutcome {
            aggregate: Self { request, state: super::ApprovalState::Amended(resolution) },
            approval: crate::ApprovedPolicyAmendment::new(
                identity, resolution.registry_revision,
            ),
        };
        proof {
            outcome.prove_model();
            super::specification::consume_amendment_refines(
                &self,
                &outcome.aggregate,
                resolution,
            );
            crate::proofs::accepted_reducer_refines(
                self.spec_model(),
                crate::model::ApprovalModelStep::ConsumeAmendment,
                outcome.spec_model(),
            );
            assert(self.spec_consume_amendment_success_is_exact(
                candidate, observed_at, &outcome,
            ));
        }
        Ok(outcome)
    }
}

} // verus!
