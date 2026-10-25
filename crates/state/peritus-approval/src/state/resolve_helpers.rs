//! Small exact helpers shared by authenticated resolution and lifecycle rejection.

use peritus_policy::{AuthorityInstant, AuthorityTimeState};

use super::types::{ApprovalState, Resolution};

const fn earliest(
    left: AuthorityInstant,
    right: AuthorityInstant,
) -> Result<AuthorityInstant, crate::ApprovalError> {
    if left.epoch().get() != right.epoch().get() {
        return Err(crate::ApprovalError::ClockEpochMismatch);
    }
    if left.tick_millis() <= right.tick_millis() { Ok(left) } else { Ok(right) }
}
#[allow(
    clippy::question_mark,
    reason = "const Result propagation requires explicit matches on stable Rust and pinned Verus"
)]
pub(super) const fn observation_expiry(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
) -> Result<AuthorityInstant, crate::ApprovalError> {
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
) -> crate::ApprovalRequest {
    let crate::ApprovalRequest {
        request_id: request_id,
        action_id: action_id,
        action_digest: action_digest,
        requester: requester,
        requester_role: requester_role,
        scope: scope,
        requirement: requirement,
        evaluated_at: evaluated_at,
        challenge_epoch: challenge_epoch,
        challenge_tick_millis: challenge_tick_millis,
        authority_time: authority_time,
        risks: risks,
        risk_details_digest: risk_details_digest,
        producing_participants: producing_participants,
        review_participants: review_participants,
        validity: validity,
        digest: digest,
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
) -> crate::ApprovalTransitionFailure {
    crate::ApprovalTransitionFailure::new(error, aggregate, observation)
}
pub(super) const fn existing_resolution(state: ApprovalState) -> Option<Resolution> {
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
