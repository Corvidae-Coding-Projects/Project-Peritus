//! Exact approver-independence predicates and executable checks.

use peritus_policy::IndependenceRequirement;
use vstd::prelude::*;

verus! {

pub(super) open spec fn requirement_is_conflicted(
    request: &crate::ApprovalRequest,
    responder: peritus_types::ActorId,
    requirement: IndependenceRequirement,
) -> bool {
    match requirement {
        IndependenceRequirement::NotRequester => super::exact::same_identifier_from(
            responder.spec_bytes(),
            request.requester.spec_bytes(),
            0,
        ),
        IndependenceRequirement::NotActionActor => super::exact::same_identifier_from(
            responder.spec_bytes(),
            request.spec_scope().spec_actor_id(),
            0,
        ),
        IndependenceRequirement::NoProducingAttemptParticipation => {
            request.producing_participants.spec_contains(responder)
        }
        IndependenceRequirement::NoReviewParticipation => {
            request.review_participants.spec_contains(responder)
        }
    }
}

pub(super) open spec fn first_violation(
    request: &crate::ApprovalRequest,
    responder: peritus_types::ActorId,
    requirements: Seq<IndependenceRequirement>,
    index: nat,
) -> bool
    decreases requirements.len() - index,
{
    if index >= requirements.len() {
        false
    } else {
        requirement_is_conflicted(request, responder, requirements[index as int])
            || first_violation(request, responder, requirements, index + 1)
    }
}

pub(super) fn requirement_is_conflicted_checked(
    request: &crate::ApprovalRequest,
    responder: peritus_types::ActorId,
    requirement: IndependenceRequirement,
) -> (result: bool)
    ensures result == requirement_is_conflicted(request, responder, requirement),
{
    proof { reveal_with_fuel(requirement_is_conflicted, 1); }
    let conflicted = match requirement {
        IndependenceRequirement::NotRequester => {
            let responder_bytes = *responder.as_bytes();
            let requester_bytes = *request.requester.as_bytes();
            let is_conflicted = super::exact::identifier_bytes_equal(
                responder_bytes,
                requester_bytes,
            );
            proof {
                assert(requirement == IndependenceRequirement::NotRequester);
                assert(requirement_is_conflicted(request, responder, requirement)
                    == super::exact::same_identifier_from(
                        responder.spec_bytes(),
                        request.requester.spec_bytes(),
                        0,
                    ));
                assert(responder_bytes == responder.spec_bytes());
                assert(requester_bytes == request.requester.spec_bytes());
                assert(is_conflicted == super::exact::same_identifier_from(
                    responder.spec_bytes(),
                    request.requester.spec_bytes(),
                    0,
                ));
                assert(is_conflicted
                    == requirement_is_conflicted(request, responder, requirement));
            }
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
    };
    conflicted
}

pub(super) fn has_violation(
    request: &crate::ApprovalRequest,
    responder: peritus_types::ActorId,
) -> (result: bool)
    ensures result == first_violation(
        request,
        responder,
        request.spec_requirement().spec_independence(),
        0,
    ),
{
    let requirements = request.requirement().independence().as_slice();
    let mut index = 0;
    while index < requirements.len()
        invariant
            0 <= index <= requirements.len(),
            requirements@ == request.spec_requirement().spec_independence(),
            first_violation(request, responder, requirements@, 0)
                == first_violation(request, responder, requirements@, index as nat),
        decreases requirements.len() - index,
    {
        let conflicted = requirement_is_conflicted_checked(
            request,
            responder,
            requirements[index],
        );
        if conflicted {
            proof {
                reveal_with_fuel(first_violation, 1);
                assert(first_violation(request, responder, requirements@, index as nat));
            }
            return true;
        }
        proof {
            reveal_with_fuel(first_violation, 1);
            assert(first_violation(request, responder, requirements@, index as nat)
                == first_violation(request, responder, requirements@, index as nat + 1));
        }
        index += 1;
    }
    false
}

} // verus!
