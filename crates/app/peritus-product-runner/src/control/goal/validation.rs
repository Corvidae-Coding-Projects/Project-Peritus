//! Goal admission boundaries and aggregate invariant validation.

use super::{
    ControlError, ControlText, GoalAdmission, GoalBudget, GoalPauseMode, GoalRecord, GoalState,
    MAX_CRITERIA,
};

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
            GoalState::BudgetReached => Ok(GoalAdmission::BudgetReached),
            GoalState::WaitingForUser | GoalState::Blocked | GoalState::Achieved => {
                Ok(GoalAdmission::Inactive)
            }
        }
    }

    pub(super) fn next_request_budget_available(&self) -> bool {
        self.usage.requests().saturating_add(self.child_budget_reservation.requests())
            < self.budget.effective_requests()
            && self
                .usage
                .active_millis
                .saturating_add(self.child_budget_reservation.active_millis())
                < self.budget.effective_active_millis()
            && self
                .usage
                .total_tokens()
                .saturating_add(self.child_budget_reservation.total_tokens())
                < self.budget.effective_tokens()
    }

    pub(super) fn usage_and_reservations_fit(&self, budget: GoalBudget) -> bool {
        self.usage
            .active_millis
            .checked_add(self.child_budget_reservation.active_millis())
            .is_some_and(|value| value <= budget.effective_active_millis())
            && self
                .usage
                .requests()
                .checked_add(self.child_budget_reservation.requests())
                .is_some_and(|value| value <= budget.effective_requests())
            && self
                .usage
                .tool_calls()
                .checked_add(self.child_budget_reservation.tool_calls())
                .is_some_and(|value| value <= budget.effective_tools())
            && self
                .usage
                .total_tokens()
                .checked_add(self.child_budget_reservation.total_tokens())
                .is_some_and(|value| value <= budget.effective_tokens())
    }

    pub(super) fn mark_paused(&mut self, now: u64) -> Result<(), ControlError> {
        self.state = GoalState::Paused;
        self.pause_mode = None;
        self.reason = ControlText::new(
            "Paused at a durable safe boundary; resume retains all cumulative accounting."
                .to_owned(),
        )?;
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(super) fn reach_budget(&mut self, now: u64) -> Result<(), ControlError> {
        self.state = GoalState::BudgetReached;
        self.pause_mode = None;
        self.reason = ControlText::new(
            "A cumulative goal limit is reached; no new operation may start.".to_owned(),
        )?;
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn validate(&self) -> Result<(), ControlError> {
        GoalBudget::new(
            self.budget.active_millis,
            self.budget.requests,
            self.budget.tool_calls,
            self.budget.total_tokens,
        )?;
        if !self.usage_and_reservations_fit(self.budget)
            || self.run == [0; 16]
            || self.user_revision == 0
            || self.required_input_generation == 0
            || self.attempt == 0
            || self.criteria.is_empty()
            || self.criteria.len() > MAX_CRITERIA
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
