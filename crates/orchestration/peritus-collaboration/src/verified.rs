//! Executable collaboration invariants and focused Verus proof roots.

use vstd::prelude::*;

mod executable;

pub use executable::{
    cancellation_dominates, causal_graph_is_valid, join_is_truthful, replay_equivalent,
    terminal_is_truthful, transition_is_legal,
};

verus! {

/// Mathematical root/parent/depth edge validity.
pub open spec fn valid_parent_edge(
    is_root: bool,
    has_parent: bool,
    depth: int,
    parent_depth: int,
    root_matches: bool,
) -> bool {
    root_matches
        && if is_root {
            !has_parent && depth == 0
        } else {
            has_parent && depth == parent_depth + 1
        }
}

/// Mathematical independent fan-out/depth/task bound conjunction.
pub open spec fn bounded_causality(
    task_count: int,
    task_limit: int,
    depth: int,
    depth_limit: int,
    fan_out: int,
    fan_out_limit: int,
) -> bool {
    0 < task_count && task_count <= task_limit
        && 0 <= depth && depth <= depth_limit
        && 0 <= fan_out && fan_out <= fan_out_limit
}

/// Mathematical contiguous causal message relation.
pub open spec fn contiguous_message(
    ordinal: int,
    predecessor_ordinal: int,
    same_task: bool,
    predecessor_present: bool,
) -> bool {
    ordinal > 0
        && if ordinal == 1 {
            !predecessor_present
        } else {
            predecessor_present && same_task && ordinal == predecessor_ordinal + 1
        }
}

/// Mathematical all-required join truth.
pub open spec fn all_required_join(required: int, successful: int, claimed: bool) -> bool {
    0 <= successful && successful <= required && (!claimed || successful == required)
}

/// Mathematical any-required join truth.
pub open spec fn any_required_join(required: int, successful: int, claimed: bool) -> bool {
    0 <= successful && successful <= required && (!claimed || (required > 0 && successful > 0))
}

/// Mathematical cancellation dominance over later success.
pub open spec fn cancellation_dominance(
    cancellation_observed: bool,
    later_success: bool,
    acknowledged_cancel: bool,
) -> bool {
    !cancellation_observed || (!later_success && (!acknowledged_cancel || !later_success))
}

/// Mathematical terminal-success truth.
pub open spec fn truthful_terminal(
    root_success: bool,
    joins_satisfied: bool,
    pending_directives: int,
    all_tasks_terminal: bool,
    completed: bool,
) -> bool {
    !completed
        || (root_success && joins_satisfied && pending_directives == 0 && all_tasks_terminal)
}

/// Mathematical exact one-event fenced transition.
#[allow(clippy::too_many_arguments)]
pub open spec fn legal_step(
    active: bool,
    current_sequence: int,
    expected_sequence: int,
    predecessor_matches: bool,
    revision_matches: bool,
    prior_digest_matches: bool,
    command_fresh: bool,
    event_count: int,
    successor_sequence: int,
) -> bool {
    active
        && current_sequence == expected_sequence
        && predecessor_matches
        && revision_matches
        && prior_digest_matches
        && command_fresh
        && event_count == 1
        && successor_sequence == current_sequence + 1
}

/// Mathematical replay equivalence.
pub open spec fn exact_replay(expected: int, observed: int, claimed: bool) -> bool {
    !claimed || expected == observed
}

/// Proves a non-root cannot legally omit its parent.
pub proof fn child_requires_parent(depth: int, parent_depth: int, root_matches: bool)
    requires depth > 0
    ensures !valid_parent_edge(false, false, depth, parent_depth, root_matches)
{
}

/// Proves a root cannot have positive depth.
pub proof fn root_depth_is_zero(depth: int, root_matches: bool)
    requires depth > 0
    ensures !valid_parent_edge(true, false, depth, 0, root_matches)
{
}

/// Proves an all-required join cannot claim success while a required child is missing.
pub proof fn all_join_rejects_missing(required: int, successful: int)
    requires 0 <= successful < required
    ensures !all_required_join(required, successful, true)
{
}

/// Proves an any-required join cannot claim success with no required success.
pub proof fn any_join_requires_success(required: int)
    requires required >= 0
    ensures !any_required_join(required, 0, true)
{
}

/// Proves cancellation excludes later success.
pub proof fn cancellation_excludes_success(acknowledged: bool)
    ensures !cancellation_dominance(true, true, acknowledged)
{
}

/// Proves pending directives exclude successful aggregate finalization.
pub proof fn pending_directive_blocks_completion(
    root_success: bool,
    joins_satisfied: bool,
    pending: int,
    all_terminal: bool,
)
    requires pending > 0
    ensures !truthful_terminal(root_success, joins_satisfied, pending, all_terminal, true)
{
}

/// Proves a claimed exact replay must have equal observations.
pub proof fn replay_claim_requires_equality(expected: int, observed: int)
    requires expected != observed
    ensures !exact_replay(expected, observed, true)
{
}

} // verus!
