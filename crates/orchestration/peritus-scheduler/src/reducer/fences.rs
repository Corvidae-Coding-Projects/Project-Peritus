//! Exact command-fence admission before semantic reduction.

use peritus_types::{CommandId, RevisionTuple};

use crate::{SchedulerCommand, SchedulerCommandKind, SchedulerPhase, SchedulerState};

#[doc = " Ordered internal classification projected to the existing public errors."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FenceAdmission {
    #[doc = " Every exact command fence is current and unused."]
    Accepted,
    #[doc = " Terminal aggregates reject every further command first."]
    Terminal,
    #[doc = " The retained command-identity history is already full."]
    HistoryLimit,
    #[doc = " At least one exact command fence is stale or illegal after genesis."]
    Stale,
}
const fn bytes_16_equal(left: &[u8; 16], right: &[u8; 16]) -> bool {
    let mut index = 0;
    while index < 16 {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
const fn bytes_32_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut index = 0;
    while index < 32 {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
pub(super) const fn revision_values_equal(left: RevisionTuple, right: RevisionTuple) -> bool {
    bytes_16_equal(left.acceptance_spec_id().as_bytes(), right.acceptance_spec_id().as_bytes())
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
fn command_id_values_contain(values: &[CommandId], target: CommandId) -> bool {
    let target_bytes = target.as_bytes();
    let mut index = 0;
    while index < values.len() {
        if bytes_16_equal(values[index].as_bytes(), target_bytes) {
            return true;
        }
        index += 1;
    }
    false
}
const fn predecessor_values_equal(state: &SchedulerState, command: &SchedulerCommand) -> bool {
    match command.expected_previous_event() {
        Some(previous) => bytes_16_equal(previous.as_bytes(), state.last_event_id().as_bytes()),
        None => false,
    }
}
const fn command_is_start(command: &SchedulerCommand) -> bool {
    matches!(command.kind(), SchedulerCommandKind::StartScheduler { .. })
}
#[doc = " Checks the production fence order before public error construction."]
pub(super) fn classify(state: &SchedulerState, command: &SchedulerCommand) -> FenceAdmission {
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
        || !bytes_32_equal(command.prior_state_digest().as_bytes(), state.state_digest().as_bytes())
        || command_id_values_contain(state.used_commands(), command.command_id())
        || command_is_start(command)
    {
        FenceAdmission::Stale
    } else {
        FenceAdmission::Accepted
    }
}
