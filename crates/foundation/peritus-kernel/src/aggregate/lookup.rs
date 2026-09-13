//! Bounded aggregate lookup and liveness queries.

use super::KernelAggregate;
use crate::{ActionState, AttemptState, ReviewState, RunState, TurnState, WaiverState};
use peritus_types::{ActionId, AttemptId, CommandId, EventId, FindingId, ReviewCycleId, RunId, TurnId};
use vstd::prelude::*;

verus! {

impl KernelAggregate {
    /// Returns a run by exact identity.
    #[must_use]
    pub fn run(&self, id: RunId) -> Option<&RunState> {
        let mut index = 0;
        while index < self.runs.len()
            invariant index <= self.runs.len(),
            decreases self.runs.len() - index,
        {
            if self.runs[index].id() == id { return Some(&self.runs[index]); }
            index += 1;
        }
        None
    }
    /// Returns an attempt by exact identity.
    #[must_use]
    pub fn attempt(&self, id: AttemptId) -> Option<&AttemptState> {
        let mut index = 0;
        while index < self.attempts.len()
            invariant index <= self.attempts.len(),
            decreases self.attempts.len() - index,
        {
            if self.attempts[index].id() == id { return Some(&self.attempts[index]); }
            index += 1;
        }
        None
    }
    /// Returns a turn by exact identity.
    #[must_use]
    pub fn turn(&self, id: TurnId) -> Option<&TurnState> {
        let mut index = 0;
        while index < self.turns.len()
            invariant index <= self.turns.len(),
            decreases self.turns.len() - index,
        {
            if self.turns[index].id() == id { return Some(&self.turns[index]); }
            index += 1;
        }
        None
    }
    /// Returns an action by exact identity.
    #[must_use]
    pub fn action(&self, id: ActionId) -> Option<&ActionState> {
        let mut index = 0;
        while index < self.actions.len()
            invariant index <= self.actions.len(),
            decreases self.actions.len() - index,
        {
            if self.actions[index].id() == id { return Some(&self.actions[index]); }
            index += 1;
        }
        None
    }
    /// Returns a review by exact cycle identity.
    #[must_use]
    pub fn review(&self, id: ReviewCycleId) -> Option<&ReviewState> {
        let mut index = 0;
        while index < self.reviews.len()
            invariant index <= self.reviews.len(),
            decreases self.reviews.len() - index,
        {
            if self.reviews[index].id() == id { return Some(&self.reviews[index]); }
            index += 1;
        }
        None
    }
    /// Returns a waiver by exact finding identity.
    #[must_use]
    pub fn waiver(&self, id: FindingId) -> Option<&WaiverState> {
        let mut index = 0;
        while index < self.waivers.len()
            invariant index <= self.waivers.len(),
            decreases self.waivers.len() - index,
        {
            if self.waivers[index].finding_id() == id { return Some(&self.waivers[index]); }
            index += 1;
        }
        None
    }

    pub(crate) fn run_index(&self, id: RunId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => {
                (index as int) < self.runs@.len()
                    && crate::identity::run_ids_equal(self.runs@[index as int].id, id)
            }
            None => forall |index: int| 0 <= index < self.runs@.len()
                ==> !crate::identity::run_ids_equal(self.runs@[index].id, id),
        },
    {
        let mut index = 0;
        while index < self.runs.len()
            invariant
                index <= self.runs.len(),
                forall |prior: int| 0 <= prior < index
                    ==> !crate::identity::run_ids_equal(self.runs@[prior].id, id),
            decreases self.runs.len() - index,
        {
            let current = &self.runs[index];
            assert(*current == self.runs@[index as int]);
            proof {
                RunState::equal_model_fields(*current, self.runs@[index as int]);
            }
            if crate::identity::run_id_equal(current.id, id) {
                return Some(index);
            }
            index += 1;
        }
        None
    }
    pub(crate) fn attempt_index(&self, id: AttemptId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => (index as int) < self.attempts@.len(),
            None => true,
        },
    {
        let mut index = 0;
        while index < self.attempts.len()
            invariant index <= self.attempts.len(),
            decreases self.attempts.len() - index,
        {
            if self.attempts[index].id() == id { return Some(index); }
            index += 1;
        }
        None
    }
    pub(crate) fn turn_index(&self, id: TurnId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => (index as int) < self.turns@.len(),
            None => true,
        },
    {
        let mut index = 0;
        while index < self.turns.len()
            invariant index <= self.turns.len(),
            decreases self.turns.len() - index,
        {
            if self.turns[index].id() == id { return Some(index); }
            index += 1;
        }
        None
    }
    pub(crate) fn action_index(&self, id: ActionId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => (index as int) < self.actions@.len(),
            None => true,
        },
    {
        let mut index = 0;
        while index < self.actions.len()
            invariant index <= self.actions.len(),
            decreases self.actions.len() - index,
        {
            if self.actions[index].id() == id { return Some(index); }
            index += 1;
        }
        None
    }
    pub(crate) fn review_index(&self, id: ReviewCycleId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => (index as int) < self.reviews@.len(),
            None => true,
        },
    {
        let mut index = 0;
        while index < self.reviews.len()
            invariant index <= self.reviews.len(),
            decreases self.reviews.len() - index,
        {
            if self.reviews[index].id() == id { return Some(index); }
            index += 1;
        }
        None
    }
    pub(crate) fn waiver_index(&self, id: FindingId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => (index as int) < self.waivers@.len(),
            None => true,
        },
    {
        let mut index = 0;
        while index < self.waivers.len()
            invariant index <= self.waivers.len(),
            decreases self.waivers.len() - index,
        {
            if self.waivers[index].finding_id() == id { return Some(index); }
            index += 1;
        }
        None
    }
    pub(crate) fn contains_command(&self, id: CommandId) -> bool {
        let mut index = 0;
        while index < self.accepted_command_ids.len()
            invariant index <= self.accepted_command_ids.len(),
            decreases self.accepted_command_ids.len() - index,
        {
            if self.accepted_command_ids[index] == id { return true; }
            index += 1;
        }
        false
    }
    pub(crate) fn contains_event(&self, id: EventId) -> bool {
        let mut index = 0;
        while index < self.event_ids.len()
            invariant index <= self.event_ids.len(),
            decreases self.event_ids.len() - index,
        {
            if self.event_ids[index] == id { return true; }
            index += 1;
        }
        false
    }
    pub(crate) fn has_live_run(&self) -> bool {
        let mut index = 0;
        while index < self.runs.len()
            invariant index <= self.runs.len(),
            decreases self.runs.len() - index,
        {
            if !self.runs[index].phase().is_terminal() { return true; }
            index += 1;
        }
        false
    }
    pub(crate) fn has_live_turn_for_attempt(&self, attempt_id: AttemptId) -> bool {
        let mut index = 0;
        while index < self.turns.len()
            invariant index <= self.turns.len(),
            decreases self.turns.len() - index,
        {
            if self.turns[index].attempt_id() == attempt_id
                && !self.turns[index].phase().is_terminal()
            { return true; }
            index += 1;
        }
        false
    }
    pub(crate) fn has_live_action_for_turn(&self, turn_id: TurnId) -> bool {
        let mut index = 0;
        while index < self.actions.len()
            invariant index <= self.actions.len(),
            decreases self.actions.len() - index,
        {
            if self.actions[index].turn_id() == turn_id
                && !self.actions[index].phase().is_terminal()
            { return true; }
            index += 1;
        }
        false
    }
}

} // verus!
