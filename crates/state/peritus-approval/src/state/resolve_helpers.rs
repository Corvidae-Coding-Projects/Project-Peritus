//! Small exact helpers shared by authenticated resolution and lifecycle rejection.

use peritus_policy::{AuthorityInstant, AuthorityTimeState};
use vstd::prelude::*;

use super::types::{ApprovalState, Resolution};

verus! {

const fn earliest(
    left: AuthorityInstant,
    right: AuthorityInstant,
) -> (result: Result<AuthorityInstant, crate::ApprovalError>)
    ensures result == super::resolve_specification::earliest_result(left, right),
{
    if left.epoch().get() != right.epoch().get() {
        return Err(crate::ApprovalError::ClockEpochMismatch);
    }
    if left.tick_millis() <= right.tick_millis() {
        Ok(left)
    } else {
        Ok(right)
    }
}

pub(super) fn observation_expiry(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
) -> (result: Result<AuthorityInstant, crate::ApprovalError>)
    ensures result == super::resolve_specification::observation_expiry_result(request, observation),
{
    let mut expiry = request.validity().expires_at();
    expiry = match earliest(expiry, request.scope().validity().expires_at()) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };
    expiry = match earliest(expiry, request.requirement().validity().expires_at()) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };
    expiry = match earliest(expiry, observation.credential_validity.expires_at()) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };
    earliest(expiry, observation.decision_expires_at)
}

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands the move-only destructure to explicit field patterns"
)]
pub(super) fn advance_time_checked(
    request: crate::ApprovalRequest,
    observed_at: AuthorityInstant,
) -> (advanced: crate::ApprovalRequest)
    requires
        observed_at.spec_epoch() == request.authority_time.spec_epoch(),
        observed_at.spec_tick_millis()
            >= request.authority_time.spec_greatest_tick_millis(),
    ensures super::exact::request_is_exact_advance(&advanced, &request, observed_at),
{
    let crate::ApprovalRequest {
        request_id,
        action_id,
        action_digest,
        requester,
        requester_role,
        scope,
        requirement,
        evaluated_at,
        challenge_epoch,
        challenge_tick_millis,
        authority_time,
        risks,
        risk_details_digest,
        producing_participants,
        review_participants,
        validity,
        digest,
    } = request;
    let _ = authority_time;
    crate::ApprovalRequest {
        request_id,
        action_id,
        action_digest,
        requester,
        requester_role,
        scope,
        requirement,
        evaluated_at,
        challenge_epoch,
        challenge_tick_millis,
        authority_time: AuthorityTimeState::new(observed_at),
        risks,
        risk_details_digest,
        producing_participants,
        review_participants,
        validity,
        digest,
    }
}

pub(super) const fn transition_failure(
    error: crate::ApprovalError,
    aggregate: super::ApprovalAggregate,
    observation: Option<crate::AuthenticatedApprovalObservation>,
) -> (failure: crate::ApprovalTransitionFailure)
    ensures
        failure.error == error,
        failure.aggregate == aggregate,
        failure.observation == observation,
        failure.spec_preserves_aggregate(&aggregate),
        failure.spec_observation() == observation,
{
    let failure = crate::ApprovalTransitionFailure::new(error, aggregate, observation);
    proof {
        failure.prove_preserves(&aggregate);
        crate::proofs::rejected_reducer_preserves(&aggregate, &failure.aggregate);
    }
    failure
}

pub(super) const fn existing_resolution(state: ApprovalState) -> (result: Option<Resolution>)
    ensures result == super::resolve_specification::retained_resolution(state),
{
    match state {
        ApprovalState::ApprovedOnce(value)
        | ApprovalState::AmendmentAuthorized(value)
        | ApprovalState::Consumed(value)
        | ApprovalState::Amended(value)
        | ApprovalState::Denied(value) => Some(value),
        ApprovalState::Expired(value) => value,
        ApprovalState::Pending | ApprovalState::Cancelled => None,
    }
}

} // verus!
