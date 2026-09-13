//! Executable D0 invariants with Verus specifications.

use vstd::prelude::*;

verus! {

/// Mathematical closed phase-transition relation over [`crate::AgentPhase::tag`] values.
pub open spec fn phase_transition_valid_spec(before: int, after: int) -> bool {
    before == after
        || (before == 0 && after == 1)
        || (before == 1 && after == 2)
        || (before == 2 && (after == 3 || after == 7))
        || (before == 3 && after == 4)
        || (before == 4 && after == 5)
        || (before == 5 && after == 6)
        || (before == 6 && after == 0)
        || (before <= 7 && after == 8)
        || (before == 8 && after <= 7)
        || (before <= 8 && after == 9)
        || (before == 9 && after == 12)
        || (before == 7 && after == 10)
        || (before <= 9 && after == 11)
}

/// Checks the only legal pure aggregate phase transitions.
#[must_use]
pub const fn phase_transition_valid(before: u8, after: u8) -> (result: bool)
    ensures result == phase_transition_valid_spec(before as int, after as int),
{
    before == after
        || (before == 0 && after == 1)
        || (before == 1 && after == 2)
        || (before == 2 && (after == 3 || after == 7))
        || (before == 3 && after == 4)
        || (before == 4 && after == 5)
        || (before == 5 && after == 6)
        || (before == 6 && after == 0)
        || (before <= 7 && after == 8)
        || (before == 8 && after <= 7)
        || (before <= 8 && after == 9)
        || (before == 9 && after == 12)
        || (before == 7 && after == 10)
        || (before <= 9 && after == 11)
}

/// Mathematical completion gate: success is never inferred from a partial or unsafe state.
pub open spec fn completion_eligible_spec(
    normal_terminal: bool,
    no_incomplete_items: bool,
    usage_settled: bool,
    no_pending_tools: bool,
    revision_fresh: bool,
    no_indeterminate_effect: bool,
) -> bool {
    normal_terminal && no_incomplete_items && usage_settled && no_pending_tools
        && revision_fresh && no_indeterminate_effect
}

/// Checks every independent completion predicate.
#[must_use]
#[allow(clippy::fn_params_excessive_bools, reason = "formal completion facts remain independent")]
pub const fn completion_eligible(
    normal_terminal: bool,
    no_incomplete_items: bool,
    usage_settled: bool,
    no_pending_tools: bool,
    revision_fresh: bool,
    no_indeterminate_effect: bool,
) -> (result: bool)
    ensures result == completion_eligible_spec(
        normal_terminal,
        no_incomplete_items,
        usage_settled,
        no_pending_tools,
        revision_fresh,
        no_indeterminate_effect,
    ),
{
    normal_terminal && no_incomplete_items && usage_settled && no_pending_tools
        && revision_fresh && no_indeterminate_effect
}

/// A proposal cannot itself dispatch an effect or accept a run.
#[must_use]
pub const fn proposal_has_no_effect(dispatches: bool, accepts: bool) -> (result: bool)
    ensures result == (!dispatches && !accepts),
{
    !dispatches && !accepts
}

/// Checked counters remain bounded without clamping.
#[must_use]
pub const fn counter_within_limit(counter: u64, limit: u64) -> (result: bool)
    ensures result == (counter <= limit),
{
    counter <= limit
}

/// Stable results preserve proposal ordinals exactly.
#[must_use]
pub const fn tool_result_order_valid(expected_ordinal: u16, actual_ordinal: u16) -> (result: bool)
    ensures result == (expected_ordinal == actual_ordinal),
{
    expected_ordinal == actual_ordinal
}

} // verus!
