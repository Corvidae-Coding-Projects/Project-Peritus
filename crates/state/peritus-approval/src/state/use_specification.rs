//! Closed total exact relations for approval-use reducers.

#[cfg(verus_only)]
use peritus_policy::{AuthorityInstant, PolicyRevisionCandidate};
#[cfg(verus_only)]
use peritus_types::ActionId;
use vstd::prelude::*;

verus! {

impl super::ApprovalAggregate {
    /// Returns the exact ordered first failure for one approve-once use attempt.
    pub(super) open spec fn spec_consume_once_expected_error(
        &self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
    ) -> Option<crate::ApprovalError> {
        match self.state {
            super::ApprovalState::ApprovedOnce(resolution) => {
                if !super::exact::same_identifier_from(
                    action_id.spec_bytes(),
                    self.request.action_id.spec_bytes(),
                    0,
                ) {
                    Some(crate::ApprovalError::BindingMismatch(
                        crate::ScopeDimension::Action,
                    ))
                } else if !super::exact::same_digest_from(
                    action_digest.spec_bytes(),
                    self.request.action_digest.spec_bytes(),
                    0,
                ) {
                    Some(crate::ApprovalError::BindingMismatch(
                        crate::ScopeDimension::ActionDigest,
                    ))
                } else if self.request.spec_observation_time_error(observed_at).is_some() {
                    self.request.spec_observation_time_error(observed_at)
                } else if observed_at.spec_epoch() != resolution.valid_until.spec_epoch()
                    || observed_at.spec_tick_millis()
                        >= resolution.valid_until.spec_tick_millis()
                {
                    Some(crate::ApprovalError::Expired)
                } else {
                    None
                }
            }
            super::ApprovalState::Consumed(_) => Some(crate::ApprovalError::AlreadyConsumed),
            _ => Some(crate::ApprovalError::IllegalPhase {
                expected: crate::ApprovalPhase::ApprovedOnce,
                actual: self.spec_phase(),
            }),
        }
    }

    /// Returns the exact complete accepted approve-once successor and authority outputs.
    pub(super) open spec fn spec_consume_once_success_is_exact(
        &self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
        outcome: &crate::ApprovalUseOutcome,
    ) -> bool {
        match self.state {
            super::ApprovalState::ApprovedOnce(resolution) => {
                super::exact::request_is_exact_advance(
                    &outcome.aggregate.request,
                    &self.request,
                    observed_at,
                )
                    && outcome.aggregate.state == super::ApprovalState::Consumed(resolution)
                    && outcome.transition.request_id == self.request.request_id
                    && outcome.transition.request_digest == self.request.digest
                    && outcome.transition.action_id == action_id
                    && outcome.transition.action_digest == action_digest
                    && outcome.transition.revision == self.request.scope.spec_revision()
                    && outcome.transition.decision_digest == resolution.decision_digest
                    && outcome.transition.command_id == resolution.command_id
                    && outcome.transition.registry_revision == resolution.registry_revision
                    && outcome.transition.valid_until == resolution.valid_until
                    && outcome.consumed.request_id == self.request.request_id
                    && outcome.consumed.decision_digest == resolution.decision_digest
                    && outcome.consumed.action_id == action_id
                    && crate::model::inv_009(outcome.spec_model())
            }
            _ => false,
        }
    }

    /// Closed total contract for every approve-once accepted or rejected result.
    pub closed spec fn spec_consume_once_result_is_exact(
        &self,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        observed_at: AuthorityInstant,
        result: &Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure>,
    ) -> bool {
        consume_once_result_relation(self, action_id, action_digest, observed_at, result)
    }

    /// Returns the exact ordered first failure for one amendment-use attempt.
    pub(super) open spec fn spec_consume_amendment_expected_error(
        &self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
    ) -> Option<crate::ApprovalError> {
        match self.state {
            super::ApprovalState::AmendmentAuthorized(resolution) => match resolution.choice {
                crate::ApprovalChoice::Amend(identity) => {
                    if !identity.spec_matches_candidate(candidate) {
                        Some(crate::ApprovalError::BindingMismatch(
                            crate::ScopeDimension::Choice,
                        ))
                    } else if self.request.spec_observation_time_error(observed_at).is_some() {
                        self.request.spec_observation_time_error(observed_at)
                    } else if observed_at.spec_epoch() != resolution.valid_until.spec_epoch()
                        || observed_at.spec_tick_millis()
                            >= resolution.valid_until.spec_tick_millis()
                    {
                        Some(crate::ApprovalError::Expired)
                    } else {
                        None
                    }
                }
                _ => Some(crate::ApprovalError::CorruptState),
            },
            _ => Some(crate::ApprovalError::IllegalPhase {
                expected: crate::ApprovalPhase::AmendmentAuthorized,
                actual: self.spec_phase(),
            }),
        }
    }

    /// Returns the exact complete accepted amendment successor and authorization output.
    pub(super) open spec fn spec_consume_amendment_success_is_exact(
        &self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
        outcome: &crate::ApprovalAmendmentOutcome,
    ) -> bool {
        match self.state {
            super::ApprovalState::AmendmentAuthorized(resolution) => match resolution.choice {
                crate::ApprovalChoice::Amend(identity) => {
                    identity.spec_matches_candidate(candidate)
                        && super::exact::request_is_exact_advance(
                            &outcome.aggregate.request,
                            &self.request,
                            observed_at,
                        )
                        && outcome.aggregate.state == super::ApprovalState::Amended(resolution)
                        && outcome.approval.identity == identity
                        && outcome.approval.registry_revision == resolution.registry_revision
                        && crate::model::inv_009(outcome.spec_model())
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Closed total contract for every amendment accepted or rejected result.
    pub closed spec fn spec_consume_amendment_result_is_exact(
        &self,
        candidate: &PolicyRevisionCandidate,
        observed_at: AuthorityInstant,
        result: &Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure>,
    ) -> bool {
        consume_amendment_result_relation(self, candidate, observed_at, result)
    }
}

pub(super) open spec fn consume_once_result_relation(
    before: &super::ApprovalAggregate,
    action_id: ActionId,
    action_digest: crate::ActionDigest,
    observed_at: AuthorityInstant,
    result: &Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure>,
) -> bool {
    match before.spec_consume_once_expected_error(action_id, action_digest, observed_at) {
        Some(expected) => match result {
            Err(failure) => failure.error == expected && failure.aggregate == *before,
            Ok(_) => false,
        },
        None => match result {
            Ok(outcome) => before.spec_consume_once_success_is_exact(
                action_id,
                action_digest,
                observed_at,
                outcome,
            ),
            Err(_) => false,
        },
    }
}

pub(super) proof fn close_consume_once_relation(
    before: &super::ApprovalAggregate,
    action_id: ActionId,
    action_digest: crate::ActionDigest,
    observed_at: AuthorityInstant,
    result: &Result<crate::ApprovalUseOutcome, crate::ApprovalUseFailure>,
)
    requires consume_once_result_relation(
        before,
        action_id,
        action_digest,
        observed_at,
        result,
    ),
    ensures before.spec_consume_once_result_is_exact(
        action_id,
        action_digest,
        observed_at,
        result,
    ),
{
}

pub(super) open spec fn consume_amendment_result_relation(
    before: &super::ApprovalAggregate,
    candidate: &PolicyRevisionCandidate,
    observed_at: AuthorityInstant,
    result: &Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure>,
) -> bool {
    match before.spec_consume_amendment_expected_error(candidate, observed_at) {
        Some(expected) => match result {
            Err(failure) => {
                failure.error == expected
                    && failure.aggregate == *before
                    && failure.observation.is_none()
            }
            Ok(_) => false,
        },
        None => match result {
            Ok(outcome) => before.spec_consume_amendment_success_is_exact(
                candidate,
                observed_at,
                outcome,
            ),
            Err(_) => false,
        },
    }
}

pub(super) proof fn close_consume_amendment_relation(
    before: &super::ApprovalAggregate,
    candidate: &PolicyRevisionCandidate,
    observed_at: AuthorityInstant,
    result: &Result<crate::ApprovalAmendmentOutcome, crate::ApprovalTransitionFailure>,
)
    requires consume_amendment_result_relation(before, candidate, observed_at, result),
    ensures before.spec_consume_amendment_result_is_exact(candidate, observed_at, result),
{
}

} // verus!
