//! Exact approver-independence predicates and executable checks.

use peritus_policy::IndependenceRequirement;

pub(super) fn requirement_is_conflicted_checked(
    request: &crate::ApprovalRequest,
    responder: peritus_types::ActorId,
    requirement: IndependenceRequirement,
) -> bool {
    match requirement {
        IndependenceRequirement::NotRequester => {
            let responder_bytes = *responder.as_bytes();
            let requester_bytes = *request.requester.as_bytes();
            let is_conflicted =
                super::exact::identifier_bytes_equal(responder_bytes, requester_bytes);
            is_conflicted
        }
        IndependenceRequirement::NotActionActor => super::exact::identifier_bytes_equal(
            *responder.as_bytes(),
            *request.scope().actor_id().as_bytes(),
        ),
        IndependenceRequirement::NoProducingAttemptParticipation => {
            request.producing_participants.contains(responder)
        }
        IndependenceRequirement::NoReviewParticipation => {
            request.review_participants.contains(responder)
        }
    }
}
pub(super) fn has_violation(
    request: &crate::ApprovalRequest,
    responder: peritus_types::ActorId,
) -> bool {
    let requirements = request.requirement().independence().as_slice();
    let mut index = 0;
    while index < requirements.len() {
        let conflicted = requirement_is_conflicted_checked(request, responder, requirements[index]);
        if conflicted {
            return true;
        }
        index += 1;
    }
    false
}
