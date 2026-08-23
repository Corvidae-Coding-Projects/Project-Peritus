//! Exact response replay and terminality lemmas.

use vstd::prelude::*;

verus! {

pub(crate) proof fn exact_replay_is_idempotent(
    state: crate::model::ApprovalModelState,
    digest: crate::ApprovalDecisionDigest,
)
    ensures
        crate::model::next(state, crate::model::ApprovalModelStep::Replay(digest)) == state,
{
}

pub(crate) proof fn terminal_state_cannot_change(
    state: crate::model::ApprovalModelState,
    step: crate::model::ApprovalModelStep,
)
    requires crate::model::terminal(state.phase),
    ensures crate::model::next(state, step) == state,
{
}

} // verus!
