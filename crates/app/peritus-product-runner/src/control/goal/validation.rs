//! Goal admission boundaries and aggregate invariant validation.

use super::{ControlError, GoalAdmission, GoalPauseMode, GoalRecord, GoalState};
use crate::control::GoalText;

impl GoalRecord {
    pub(super) fn boundary(
        &mut self,
        operation_completed: bool,
        now: u64,
    ) -> Result<GoalAdmission, ControlError> {
        match self.state {
            GoalState::Active => Ok(GoalAdmission::Accepted),
            GoalState::Pausing => match self.pause_mode {
                Some(GoalPauseMode::BeforeEdit) => Ok(GoalAdmission::Accepted),
                Some(GoalPauseMode::AfterOperation) if !operation_completed => {
                    self.mark_paused(now)?;
                    Ok(GoalAdmission::Paused)
                }
                Some(GoalPauseMode::AfterOperation | GoalPauseMode::Now) => {
                    self.mark_paused(now)?;
                    Ok(GoalAdmission::Paused)
                }
                None => Err(ControlError::InvalidInput),
            },
            GoalState::Paused | GoalState::Cancelled => Ok(GoalAdmission::Paused),
            GoalState::WaitingForUser | GoalState::Blocked | GoalState::Achieved => {
                Ok(GoalAdmission::Inactive)
            }
        }
    }

    pub(super) fn mark_paused(&mut self, now: u64) -> Result<(), ControlError> {
        self.state = GoalState::Paused;
        self.pause_mode = None;
        self.reason = GoalText::new(
            "Paused at a durable safe boundary; resume retains all cumulative accounting."
                .to_owned(),
        )?;
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn validate(&self) -> Result<(), ControlError> {
        if self.run == [0; 16]
            || self.user_revision == 0
            || self.required_input_generation == 0
            || self.attempt == 0
            || self.criteria.is_empty()
            || (self.state == GoalState::Pausing) != self.pause_mode.is_some()
            || self.usage.roles.iter().any(|usage| {
                usage.completed_requests > usage.requests
                    || usage.token_reported_requests > usage.completed_requests
                    || usage.cost_reported_requests > usage.completed_requests
            })
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}
