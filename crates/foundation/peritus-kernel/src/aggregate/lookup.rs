//! Bounded aggregate lookup and liveness queries.

use super::KernelAggregate;
use crate::{ActionState, AttemptState, ReviewState, RunState, TurnState, WaiverState};
use peritus_types::{
    ActionId, AttemptId, CommandId, EventId, FindingId, ReviewCycleId, RunId, TurnId,
};

impl KernelAggregate {
    #[doc = " Returns a run by exact identity."]
    #[must_use]
    pub fn run(&self, id: RunId) -> Option<&RunState> {
        let mut index = 0;
        while index < self.runs.len() {
            if self.runs[index].id() == id {
                return Some(&self.runs[index]);
            }
            index += 1;
        }
        None
    }
    #[doc = " Returns an attempt by exact identity."]
    #[must_use]
    pub fn attempt(&self, id: AttemptId) -> Option<&AttemptState> {
        let mut index = 0;
        while index < self.attempts.len() {
            if self.attempts[index].id() == id {
                return Some(&self.attempts[index]);
            }
            index += 1;
        }
        None
    }
    #[doc = " Returns a turn by exact identity."]
    #[must_use]
    pub fn turn(&self, id: TurnId) -> Option<&TurnState> {
        let mut index = 0;
        while index < self.turns.len() {
            if self.turns[index].id() == id {
                return Some(&self.turns[index]);
            }
            index += 1;
        }
        None
    }
    #[doc = " Returns an action by exact identity."]
    #[must_use]
    pub fn action(&self, id: ActionId) -> Option<&ActionState> {
        let mut index = 0;
        while index < self.actions.len() {
            if self.actions[index].id() == id {
                return Some(&self.actions[index]);
            }
            index += 1;
        }
        None
    }
    #[doc = " Returns a review by exact cycle identity."]
    #[must_use]
    pub fn review(&self, id: ReviewCycleId) -> Option<&ReviewState> {
        let mut index = 0;
        while index < self.reviews.len() {
            if self.reviews[index].id() == id {
                return Some(&self.reviews[index]);
            }
            index += 1;
        }
        None
    }
    #[doc = " Returns a waiver by exact finding identity."]
    #[must_use]
    pub fn waiver(&self, id: FindingId) -> Option<&WaiverState> {
        let mut index = 0;
        while index < self.waivers.len() {
            if self.waivers[index].finding_id() == id {
                return Some(&self.waivers[index]);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn run_index(&self, id: RunId) -> Option<usize> {
        let mut index = 0;
        while index < self.runs.len() {
            let current = &self.runs[index];
            if crate::identity::run_id_equal(current.id, id) {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn attempt_index(&self, id: AttemptId) -> Option<usize> {
        let mut index = 0;
        while index < self.attempts.len() {
            if self.attempts[index].id() == id {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn turn_index(&self, id: TurnId) -> Option<usize> {
        let mut index = 0;
        while index < self.turns.len() {
            if self.turns[index].id() == id {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn action_index(&self, id: ActionId) -> Option<usize> {
        let mut index = 0;
        while index < self.actions.len() {
            if self.actions[index].id() == id {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn review_index(&self, id: ReviewCycleId) -> Option<usize> {
        let mut index = 0;
        while index < self.reviews.len() {
            let review = self.reviews[index];
            if crate::identity::review_cycle_id_equal(review.id(), id) {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn waiver_index(&self, id: FindingId) -> Option<usize> {
        let mut index = 0;
        while index < self.waivers.len() {
            let waiver = self.waivers[index];
            if crate::identity::finding_id_equal(waiver.finding_id(), id) {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn contains_command(&self, id: CommandId) -> bool {
        let mut index = 0;
        while index < self.accepted_command_ids.len() {
            if self.accepted_command_ids[index] == id {
                return true;
            }
            index += 1;
        }
        false
    }
    pub(crate) fn contains_event(&self, id: EventId) -> bool {
        let mut index = 0;
        while index < self.event_ids.len() {
            if self.event_ids[index] == id {
                return true;
            }
            index += 1;
        }
        false
    }
    pub(crate) fn has_live_run(&self) -> bool {
        let mut index = 0;
        while index < self.runs.len() {
            if !self.runs[index].phase().is_terminal() {
                return true;
            }
            index += 1;
        }
        false
    }
    pub(crate) fn has_live_turn_for_attempt(&self, attempt_id: AttemptId) -> bool {
        let mut index = 0;
        while index < self.turns.len() {
            if self.turns[index].attempt_id() == attempt_id
                && !self.turns[index].phase().is_terminal()
            {
                return true;
            }
            index += 1;
        }
        false
    }
    pub(crate) fn has_live_action_for_turn(&self, turn_id: TurnId) -> bool {
        let mut index = 0;
        while index < self.actions.len() {
            if self.actions[index].turn_id() == turn_id
                && !self.actions[index].phase().is_terminal()
            {
                return true;
            }
            index += 1;
        }
        false
    }
}
