//! Executable aggregate invariant checks.

use super::KernelAggregate;
use crate::{AcceptancePhase, RunPhase};

pub(super) fn is_valid(state: &KernelAggregate) -> bool {
    if state.contract_binding.revision() != state.revision
        || state.contract_binding.contract_id() != state.revision.acceptance_spec_id()
        || state.accepted_command_ids.len() != state.event_ids.len()
        || state.event_ids.is_empty()
        || state.event_ids[state.event_ids.len() - 1] != state.head_event_id
    {
        return false;
    }
    if has_duplicate_commands(state) || has_duplicate_events(state) || has_duplicate_children(state)
    {
        return false;
    }
    let mut live_runs = 0usize;
    let mut run_index = 0;
    while run_index < state.runs.len() {
        let run = &state.runs[run_index];
        if run.revision() != state.revision {
            return false;
        }
        if !run.phase().is_terminal() {
            live_runs += 1;
        }
        if (run.phase() == RunPhase::Accepted) != (run.acceptance() == AcceptancePhase::Accepted) {
            return false;
        }
        if run.phase().is_terminal()
            && run.phase() != RunPhase::Accepted
            && run.acceptance() != AcceptancePhase::Terminated
        {
            return false;
        }
        run_index += 1;
    }
    if live_runs > 1 || (state.session.phase().is_terminal() && live_runs != 0) {
        return false;
    }
    parents_are_valid(state)
}
fn has_duplicate_commands(state: &KernelAggregate) -> bool {
    let mut left = 0;
    while left < state.accepted_command_ids.len() {
        let mut right = left + 1;
        while right < state.accepted_command_ids.len() {
            if state.accepted_command_ids[left] == state.accepted_command_ids[right] {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    false
}
fn has_duplicate_events(state: &KernelAggregate) -> bool {
    let mut left = 0;
    while left < state.event_ids.len() {
        let mut right = left + 1;
        while right < state.event_ids.len() {
            if state.event_ids[left] == state.event_ids[right] {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    false
}
fn has_duplicate_children(state: &KernelAggregate) -> bool {
    let mut left = 0;
    while left < state.runs.len() {
        let mut right = left + 1;
        while right < state.runs.len() {
            if state.runs[left].id() == state.runs[right].id() {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    let mut left = 0;
    while left < state.attempts.len() {
        let mut right = left + 1;
        while right < state.attempts.len() {
            if state.attempts[left].id() == state.attempts[right].id() {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    let mut left = 0;
    while left < state.turns.len() {
        let mut right = left + 1;
        while right < state.turns.len() {
            if state.turns[left].id() == state.turns[right].id() {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    let mut left = 0;
    while left < state.actions.len() {
        let mut right = left + 1;
        while right < state.actions.len() {
            if state.actions[left].id() == state.actions[right].id() {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    let mut left = 0;
    while left < state.reviews.len() {
        let mut right = left + 1;
        while right < state.reviews.len() {
            if state.reviews[left].id() == state.reviews[right].id() {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    let mut left = 0;
    while left < state.waivers.len() {
        let mut right = left + 1;
        while right < state.waivers.len() {
            if state.waivers[left].finding_id() == state.waivers[right].finding_id() {
                return true;
            }
            right += 1;
        }
        left += 1;
    }
    false
}
fn parents_are_valid(state: &KernelAggregate) -> bool {
    let mut index = 0;
    while index < state.attempts.len() {
        if state.run(state.attempts[index].run_id()).is_none() {
            return false;
        }
        index += 1;
    }
    let mut index = 0;
    while index < state.turns.len() {
        if state.attempt(state.turns[index].attempt_id()).is_none() {
            return false;
        }
        index += 1;
    }
    let mut index = 0;
    while index < state.actions.len() {
        if state.turn(state.actions[index].turn_id()).is_none() {
            return false;
        }
        index += 1;
    }
    let mut index = 0;
    while index < state.reviews.len() {
        let review = state.reviews[index];
        let Some(attempt) = state.attempt(review.attempt_id()) else {
            return false;
        };
        if attempt.run_id() != review.run_id() {
            return false;
        }
        index += 1;
    }
    let mut index = 0;
    while index < state.waivers.len() {
        let waiver = state.waivers[index];
        let Some(review) = state.review(waiver.review_cycle_id()) else {
            return false;
        };
        if review.run_id() != waiver.run_id() {
            return false;
        }
        index += 1;
    }
    true
}
