//! Exact command-fence admission before semantic reduction.

use peritus_types::{CommandId, RevisionTuple};
use vstd::prelude::*;

use crate::{SchedulerCommand, SchedulerCommandKind, SchedulerPhase, SchedulerState};

verus! {

/// Ordered internal classification projected to the existing public errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FenceAdmission {
    /// Every exact command fence is current and unused.
    Accepted,
    /// Terminal aggregates reject every further command first.
    Terminal,
    /// The retained command-identity history is already full.
    HistoryLimit,
    /// At least one exact command fence is stale or illegal after genesis.
    Stale,
}

pub open spec fn same_bytes_16(left: [u8; 16], right: [u8; 16]) -> bool {
    left == right
}

pub open spec fn same_bytes_32(left: [u8; 32], right: [u8; 32]) -> bool {
    left == right
}

const fn bytes_16_equal(left: &[u8; 16], right: &[u8; 16]) -> (equal: bool)
    ensures equal == same_bytes_16(*left, *right),
{
    let mut index = 0;
    while index < 16
        invariant
            index <= 16,
            forall |prior: int| 0 <= prior < index ==>
                left[prior] == right[prior],
        decreases 16 - index,
    {
        if left[index] != right[index] {
            assert(*left != *right);
            return false;
        }
        index += 1;
    }
    assert(left@ =~= right@) by {
        assert forall |at: int| 0 <= at < left@.len()
            implies left@[at] == right@[at] by {
        }
    }
    assert(*left == *right);
    true
}

const fn bytes_32_equal(left: &[u8; 32], right: &[u8; 32]) -> (equal: bool)
    ensures equal == same_bytes_32(*left, *right),
{
    let mut index = 0;
    while index < 32
        invariant
            index <= 32,
            forall |prior: int| 0 <= prior < index ==>
                left[prior] == right[prior],
        decreases 32 - index,
    {
        if left[index] != right[index] {
            assert(*left != *right);
            return false;
        }
        index += 1;
    }
    assert(left@ =~= right@) by {
        assert forall |at: int| 0 <= at < left@.len()
            implies left@[at] == right@[at] by {
        }
    }
    assert(*left == *right);
    true
}

/// Exact componentwise immutable revision equality.
pub open spec fn revisions_match(left: RevisionTuple, right: RevisionTuple) -> bool {
    same_bytes_16(
        left.spec_acceptance_spec_id().spec_bytes(),
        right.spec_acceptance_spec_id().spec_bytes(),
    )
        && same_bytes_16(
            left.spec_harness_id().spec_bytes(),
            right.spec_harness_id().spec_bytes(),
        )
        && same_bytes_16(
            left.spec_workspace_id().spec_bytes(),
            right.spec_workspace_id().spec_bytes(),
        )
        && left.spec_workspace_generation().spec_value()
            == right.spec_workspace_generation().spec_value()
        && left.spec_workspace_revision().spec_value()
            == right.spec_workspace_revision().spec_value()
        && same_bytes_16(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        )
        && same_bytes_16(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        )
}

pub(super) const fn revision_values_equal(
    left: RevisionTuple,
    right: RevisionTuple,
) -> (equal: bool)
    ensures equal == revisions_match(left, right),
{
    bytes_16_equal(
        left.acceptance_spec_id().as_bytes(),
        right.acceptance_spec_id().as_bytes(),
    )
        && bytes_16_equal(left.harness_id().as_bytes(), right.harness_id().as_bytes())
        && bytes_16_equal(left.workspace_id().as_bytes(), right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes_16_equal(left.policy_id().as_bytes(), right.policy_id().as_bytes())
        && bytes_16_equal(
            left.provider_profile_id().as_bytes(),
            right.provider_profile_id().as_bytes(),
        )
}

/// Exact byte identity membership in retained command history.
pub open spec fn command_id_is_used(values: Seq<CommandId>, target: CommandId) -> bool {
    exists |index: int| 0 <= index < values.len()
        && #[trigger] same_bytes_16(values[index].spec_bytes(), target.spec_bytes())
}

fn command_id_values_contain(values: &[CommandId], target: CommandId) -> (found: bool)
    ensures found == command_id_is_used(values@, target),
{
    let target_bytes = target.as_bytes();
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            *target_bytes == target.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==>
                !#[trigger] same_bytes_16(values@[prior].spec_bytes(), target.spec_bytes()),
        decreases values@.len() - index,
    {
        if bytes_16_equal(values[index].as_bytes(), target_bytes) {
            assert(command_id_is_used(values@, target)) by {
                assert(exists |found_at: int| found_at == index
                    && 0 <= found_at < values@.len()
                    && #[trigger] same_bytes_16(
                        values@[found_at].spec_bytes(),
                        target.spec_bytes(),
                    ));
            }
            return true;
        }
        index += 1;
    }
    false
}

pub open spec fn predecessor_matches(state: &SchedulerState, command: &SchedulerCommand) -> bool {
    match command.spec_expected_previous_event() {
        Some(previous) => same_bytes_16(
            previous.spec_bytes(),
            state.spec_last_event_id().spec_bytes(),
        ),
        None => false,
    }
}

const fn predecessor_values_equal(
    state: &SchedulerState,
    command: &SchedulerCommand,
) -> (equal: bool)
    ensures equal == predecessor_matches(state, command),
{
    match command.expected_previous_event() {
        Some(previous) => bytes_16_equal(
            previous.as_bytes(),
            state.last_event_id().as_bytes(),
        ),
        None => false,
    }
}

pub open spec fn is_start_command(command: &SchedulerCommand) -> bool {
    matches!(command.spec_kind(), SchedulerCommandKind::StartScheduler { .. })
}

const fn command_is_start(command: &SchedulerCommand) -> (is_start: bool)
    ensures is_start == is_start_command(command),
{
    matches!(command.kind(), SchedulerCommandKind::StartScheduler { .. })
}

/// Every exact non-genesis command fence agrees with authoritative state.
pub open spec fn fences_match(state: &SchedulerState, command: &SchedulerCommand) -> bool {
    &&& state.spec_binding().spec_semantics() == command.spec_semantics()
    &&& same_bytes_16(
        state.spec_binding().spec_run_id().spec_bytes(),
        command.spec_run_id().spec_bytes(),
    )
    &&& revisions_match(
        state.spec_binding().spec_revision(),
        command.spec_revision(),
    )
    &&& state.spec_sequence().spec_value()
        == command.spec_expected_sequence() as int
    &&& predecessor_matches(state, command)
    &&& same_bytes_32(
        command.spec_prior_state_digest().spec_bytes(),
        state.spec_state_digest().spec_bytes(),
    )
    &&& !command_id_is_used(state.spec_used_commands(), command.spec_command_id())
    &&& !is_start_command(command)
}

/// Exact ordered fence result, including terminal and history-limit priority.
pub(super) open spec fn spec_admission(
    state: &SchedulerState,
    command: &SchedulerCommand,
) -> FenceAdmission {
    if state.spec_phase() == SchedulerPhase::Terminal {
        FenceAdmission::Terminal
    } else if state.spec_used_commands().len() >= 65_535 {
        FenceAdmission::HistoryLimit
    } else if !fences_match(state, command) {
        FenceAdmission::Stale
    } else {
        FenceAdmission::Accepted
    }
}

/// Checks the production fence order before public error construction.
pub(super) fn classify(
    state: &SchedulerState,
    command: &SchedulerCommand,
) -> (result: FenceAdmission)
    ensures result == spec_admission(state, command),
{
    if matches!(state.phase(), SchedulerPhase::Terminal) {
        return FenceAdmission::Terminal;
    }
    if state.used_commands().len() >= 65_535 {
        return FenceAdmission::HistoryLimit;
    }
    if !state.binding().semantics().same(command.semantics())
        || !bytes_16_equal(state.run_id().as_bytes(), command.run_id().as_bytes())
        || !revision_values_equal(state.binding().revision(), command.revision())
        || state.sequence().get() != command.expected_sequence()
        || !predecessor_values_equal(state, command)
        || !bytes_32_equal(
            command.prior_state_digest().as_bytes(),
            state.state_digest().as_bytes(),
        )
        || command_id_values_contain(state.used_commands(), command.command_id())
        || command_is_start(command)
    {
        FenceAdmission::Stale
    } else {
        FenceAdmission::Accepted
    }
}

} // verus!
