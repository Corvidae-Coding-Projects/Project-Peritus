//! INV-009 preservation across all total-model traces.

use vstd::prelude::*;

verus! {

pub(crate) proof fn next_preserves_inv_009(
    state: crate::model::ApprovalModelState,
    step: crate::model::ApprovalModelStep,
)
    requires crate::model::inv_009(state),
    ensures crate::model::inv_009(crate::model::next(state, step)),
{
}

pub(crate) proof fn reachable_preserves_inv_009(
    state: crate::model::ApprovalModelState,
    trace: Seq<crate::model::ApprovalModelStep>,
)
    requires crate::model::inv_009(state),
    ensures crate::model::inv_009(crate::model::reachable(state, trace)),
    decreases trace.len(),
{
    if trace.len() > 0 {
        let prefix = trace.subrange(0, trace.len() - 1);
        reachable_preserves_inv_009(state, prefix);
        next_preserves_inv_009(
            crate::model::reachable(state, prefix),
            trace[trace.len() - 1],
        );
    }
}

pub(crate) proof fn initial_satisfies_inv_009()
    ensures crate::model::inv_009(crate::model::initial()),
{
}

pub(crate) proof fn accepted_reducer_refines(
    before: crate::model::ApprovalModelState,
    step: crate::model::ApprovalModelStep,
    after: crate::model::ApprovalModelState,
)
    requires
        crate::model::inv_009(before),
        after == crate::model::next(before, step),
    ensures
        after == crate::model::next(before, step),
        crate::model::inv_009(after),
{
    next_preserves_inv_009(before, step);
}

pub(crate) proof fn rejected_reducer_preserves(
    before: &crate::ApprovalAggregate,
    after: &crate::ApprovalAggregate,
)
    requires after == before,
    ensures after == before,
{
}

} // verus!
