//! Digest-binding and exact one-time-use lemmas.

use vstd::prelude::*;

verus! {

pub(crate) proof fn approve_once_binds_decision(
    state: crate::model::ApprovalModelState,
    digest: crate::ApprovalDecisionDigest,
)
    requires state.phase == crate::ApprovalPhase::Pending,
    ensures ({
        let next = crate::model::next(
            state,
            crate::model::ApprovalModelStep::ResolveApproveOnce(digest),
        );
        next.decision == Some(digest)
            && next.phase == crate::ApprovalPhase::ApprovedOnce
            && next.resolution_count == 1
    }),
{
}

pub(crate) proof fn consume_once_preserves_digest(
    state: crate::model::ApprovalModelState,
)
    requires
        state.phase == crate::ApprovalPhase::ApprovedOnce,
        state.resolution_count == 1,
    ensures ({
        let next = crate::model::next(state, crate::model::ApprovalModelStep::ConsumeOnce);
        next.decision == state.decision
            && next.phase == crate::ApprovalPhase::Consumed
            && next.use_count == 1
    }),
{
}

} // verus!
