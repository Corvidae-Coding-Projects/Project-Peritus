//! Shared exact value relations for public reducer contracts.

#[cfg(verus_only)]
use peritus_policy::AuthorityInstant;
use peritus_types::ActionId;
use vstd::prelude::*;

verus! {

pub open spec fn same_identifier_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_identifier_from(left, right, index + 1)
    }
}

pub open spec fn same_digest_from(
    left: [u8; 32],
    right: [u8; 32],
    index: nat,
) -> bool
    decreases 32 - index,
{
    if index >= 32 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_digest_from(left, right, index + 1)
    }
}

const fn identifier_values_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (result: bool)
    requires index <= 16,
    ensures result == same_identifier_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}

pub const fn identifier_bytes_equal(
    left: [u8; 16],
    right: [u8; 16],
) -> (result: bool)
    ensures result == same_identifier_from(left, right, 0),
{
    identifier_values_equal_from(left, right, 0)
}

const fn digest_values_equal_from(
    left: [u8; 32],
    right: [u8; 32],
    index: usize,
) -> (result: bool)
    requires index <= 32,
    ensures result == same_digest_from(left, right, index as nat),
    decreases 32 - index,
{
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        digest_values_equal_from(left, right, index + 1)
    }
}

pub const fn digest_bytes_equal(
    left: [u8; 32],
    right: [u8; 32],
) -> (result: bool)
    ensures result == same_digest_from(left, right, 0),
{
    digest_values_equal_from(left, right, 0)
}

pub(super) const fn action_id_values_equal(
    left: ActionId,
    right: ActionId,
) -> (result: bool)
    ensures result == same_identifier_from(left.spec_bytes(), right.spec_bytes(), 0),
{
    identifier_values_equal_from(*left.as_bytes(), *right.as_bytes(), 0)
}

pub(super) const fn action_digest_values_equal(
    left: crate::ActionDigest,
    right: crate::ActionDigest,
) -> (result: bool)
    ensures result == same_digest_from(left.spec_bytes(), right.spec_bytes(), 0),
{
    digest_values_equal_from(
        *left.sha256().as_bytes(),
        *right.sha256().as_bytes(),
        0,
    )
}

pub(super) open spec fn observation_time_error(
    request: &crate::ApprovalRequest,
    observed_at: AuthorityInstant,
) -> Option<crate::ApprovalError> {
    request.spec_observation_time_error(observed_at)
}

pub(super) open spec fn request_is_exact_advance(
    after: &crate::ApprovalRequest,
    before: &crate::ApprovalRequest,
    observed_at: AuthorityInstant,
) -> bool {
    after.request_id == before.request_id
        && after.action_id == before.action_id
        && after.action_digest == before.action_digest
        && after.requester == before.requester
        && after.requester_role == before.requester_role
        && after.scope == before.scope
        && after.requirement == before.requirement
        && after.evaluated_at == before.evaluated_at
        && after.challenge_epoch == before.challenge_epoch
        && after.challenge_tick_millis == before.challenge_tick_millis
        && after.risks == before.risks
        && after.risk_details_digest == before.risk_details_digest
        && after.producing_participants == before.producing_participants
        && after.review_participants == before.review_participants
        && after.validity == before.validity
        && after.digest == before.digest
        && after.authority_time.spec_epoch() == observed_at.spec_epoch()
        && after.authority_time.spec_greatest_tick_millis()
            == observed_at.spec_tick_millis()
}

pub(super) open spec fn state_phase(state: super::ApprovalState) -> crate::ApprovalPhase {
    match state {
        super::ApprovalState::Pending => crate::ApprovalPhase::Pending,
        super::ApprovalState::ApprovedOnce(_) => crate::ApprovalPhase::ApprovedOnce,
        super::ApprovalState::AmendmentAuthorized(_) => {
            crate::ApprovalPhase::AmendmentAuthorized
        }
        super::ApprovalState::Consumed(_) => crate::ApprovalPhase::Consumed,
        super::ApprovalState::Amended(_) => crate::ApprovalPhase::Amended,
        super::ApprovalState::Denied(_) => crate::ApprovalPhase::Denied,
        super::ApprovalState::Expired(_) => crate::ApprovalPhase::Expired,
        super::ApprovalState::Cancelled => crate::ApprovalPhase::Cancelled,
    }
}

} // verus!
