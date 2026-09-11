//! Goal construction, inspection, and user-controlled lifecycle transitions.

use super::{
    ControlError, ControlText, GoalAttemptProgress, GoalBudget, GoalCriterion, GoalCriterionKind,
    GoalCriterionState, GoalPauseMode, GoalRecord, GoalState, GoalUsage, MAX_CRITERIA, OperationId,
};

impl GoalRecord {
    pub(in crate::control) fn start(
        id: OperationId,
        run: [u8; 16],
        objective: ControlText<8192>,
        criteria: Vec<GoalCriterion>,
        budget: GoalBudget,
        required_input_generation: u64,
        now: u64,
    ) -> Result<Self, ControlError> {
        if run == [0; 16]
            || required_input_generation == 0
            || criteria.is_empty()
            || criteria.len() > MAX_CRITERIA
            || !criteria.iter().any(|criterion| {
                criterion.mandatory && criterion.kind == GoalCriterionKind::RunnerAcceptance
            })
        {
            return Err(ControlError::InvalidInput);
        }
        GoalBudget::new(
            budget.active_millis,
            budget.requests,
            budget.tool_calls,
            budget.total_tokens,
        )?;
        let goal = Self {
            id,
            run,
            objective,
            criteria,
            budget,
            state: GoalState::Active,
            reason: ControlText::new(
                "Goal confirmed; awaiting the next admitted operation.".to_owned(),
            )?,
            user_revision: 1,
            required_input_generation,
            attempt: 1,
            pause_mode: None,
            usage: GoalUsage::default(),
            child_budget_reservation: crate::control::ChildBudgetReservation::default(),
            attempt_progress: GoalAttemptProgress::default(),
            created_unix_millis: now,
            updated_unix_millis: now,
        };
        goal.validate()?;
        Ok(goal)
    }

    /// Goal identity, equal to its original explicit start operation.
    #[must_use]
    pub const fn id(&self) -> OperationId {
        self.id
    }
    /// Existing product-run identity used for every attempt.
    #[must_use]
    pub const fn run_bytes(&self) -> &[u8; 16] {
        &self.run
    }
    /// Exact user-confirmed objective.
    #[must_use]
    pub fn objective(&self) -> &str {
        self.objective.as_str()
    }
    /// Immutable typed criteria and current evidence states.
    #[must_use]
    pub fn criteria(&self) -> &[GoalCriterion] {
        &self.criteria
    }
    /// Current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> GoalState {
        self.state
    }
    /// Exact durable transition reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }
    /// User-command revision; host accounting events do not change this counter.
    #[must_use]
    pub const fn user_revision(&self) -> u64 {
        self.user_revision
    }
    /// Current cumulative user limits.
    #[must_use]
    pub const fn budget(&self) -> GoalBudget {
        self.budget
    }
    /// Current cumulative usage and unresolved-reporting provenance.
    #[must_use]
    pub const fn usage(&self) -> GoalUsage {
        self.usage
    }
    /// Capacity assigned to child branches and no longer spendable by this source goal.
    #[must_use]
    pub const fn child_budget_reservation(&self) -> crate::control::ChildBudgetReservation {
        self.child_budget_reservation
    }
    /// One-based current attempt; resume increments without replacing the goal or usage.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }
    /// Pending pause boundary, when state is `pausing`.
    #[must_use]
    pub const fn pause_mode(&self) -> Option<GoalPauseMode> {
        self.pause_mode
    }
    /// Input generation against which current completion evidence must be fresh.
    #[must_use]
    pub const fn required_input_generation(&self) -> u64 {
        self.required_input_generation
    }
    /// Stable creation wall-clock observation.
    #[must_use]
    pub const fn created_unix_millis(&self) -> u64 {
        self.created_unix_millis
    }
    /// Last durable transition observation.
    #[must_use]
    pub const fn updated_unix_millis(&self) -> u64 {
        self.updated_unix_millis
    }
    /// Whether crash recovery may revalidate and continue without explicit user resume.
    #[must_use]
    pub fn restart_eligible(&self) -> bool {
        self.state == GoalState::Active && self.next_request_budget_available()
    }
    /// Remaining active execution time for one new runner attempt.
    #[must_use]
    pub const fn remaining_active_millis(&self) -> u64 {
        self.budget
            .effective_active_millis()
            .saturating_sub(self.usage.active_millis)
            .saturating_sub(self.child_budget_reservation.active_millis())
    }

    pub(in crate::control) fn reserve_child_budget(
        &mut self,
        allocation: crate::control::ChildBudgetAllocation,
        now: u64,
    ) -> Result<(), ControlError> {
        let reserved = self.child_budget_reservation.checked_add(allocation)?;
        if self
            .usage
            .active_millis
            .checked_add(reserved.active_millis())
            .is_none_or(|value| value > self.budget.effective_active_millis())
            || self
                .usage
                .requests()
                .checked_add(reserved.requests())
                .is_none_or(|value| value > self.budget.effective_requests())
            || self
                .usage
                .tool_calls()
                .checked_add(reserved.tool_calls())
                .is_none_or(|value| value > self.budget.effective_tools())
            || self
                .usage
                .total_tokens()
                .checked_add(reserved.total_tokens())
                .is_none_or(|value| value > self.budget.effective_tokens())
        {
            return Err(ControlError::Capacity);
        }
        self.child_budget_reservation = reserved;
        self.user_revision = self.user_revision.checked_add(1).ok_or(ControlError::Capacity)?;
        self.reason = ControlText::new(
            "A child branch budget is reserved under this goal's existing cumulative ceiling."
                .to_owned(),
        )?;
        self.updated_unix_millis = now;
        if self.state == GoalState::Active && !self.next_request_budget_available() {
            self.reach_budget(now)?;
        }
        Ok(())
    }

    pub(in crate::control) fn pause(
        &mut self,
        mode: GoalPauseMode,
        now: u64,
    ) -> Result<(), ControlError> {
        if !matches!(self.state, GoalState::Active | GoalState::WaitingForUser) {
            return Err(ControlError::InvalidInput);
        }
        self.user_revision = self.user_revision.checked_add(1).ok_or(ControlError::Capacity)?;
        self.pause_mode = Some(mode);
        self.state = if self.state == GoalState::WaitingForUser {
            GoalState::Paused
        } else {
            GoalState::Pausing
        };
        self.reason = ControlText::new(
            match self.state {
                GoalState::Paused => "Paused at the existing idle boundary.",
                GoalState::Pausing => {
                    "Pause durably requested; waiting for the selected safe boundary."
                }
                _ => unreachable!(),
            }
            .to_owned(),
        )?;
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn resume(&mut self, now: u64) -> Result<(), ControlError> {
        if !matches!(
            self.state,
            GoalState::Paused
                | GoalState::WaitingForUser
                | GoalState::Blocked
                | GoalState::BudgetReached
        ) || !self.next_request_budget_available()
        {
            return Err(ControlError::InvalidInput);
        }
        self.user_revision = self.user_revision.checked_add(1).ok_or(ControlError::Capacity)?;
        self.attempt = self.attempt.checked_add(1).ok_or(ControlError::Capacity)?;
        self.attempt_progress = GoalAttemptProgress::default();
        self.state = GoalState::Active;
        self.pause_mode = None;
        self.reason =
            ControlText::new("Explicitly resumed with cumulative accounting retained.".to_owned())?;
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn update_budget(
        &mut self,
        budget: GoalBudget,
        now: u64,
    ) -> Result<(), ControlError> {
        GoalBudget::new(
            budget.active_millis,
            budget.requests,
            budget.tool_calls,
            budget.total_tokens,
        )?;
        if !self.usage_and_reservations_fit(budget) {
            return Err(ControlError::InvalidInput);
        }
        self.user_revision = self.user_revision.checked_add(1).ok_or(ControlError::Capacity)?;
        self.budget = budget;
        if self.state == GoalState::Active && !self.next_request_budget_available() {
            self.state = GoalState::BudgetReached;
            self.reason = ControlText::new(
                "Updated limit is already reached; no new operation may start.".to_owned(),
            )?;
        } else {
            self.reason = ControlText::new(
                "Budget updated; increasing a limit does not resume execution.".to_owned(),
            )?;
        }
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn cancel(&mut self, now: u64) -> Result<(), ControlError> {
        if matches!(self.state, GoalState::Achieved | GoalState::Cancelled) {
            return Err(ControlError::InvalidInput);
        }
        self.user_revision = self.user_revision.checked_add(1).ok_or(ControlError::Capacity)?;
        self.state = GoalState::Cancelled;
        self.pause_mode = None;
        self.reason = ControlText::new(
            "Goal continuation cancelled; completed effects and history are retained.".to_owned(),
        )?;
        self.updated_unix_millis = now;
        Ok(())
    }

    pub(in crate::control) fn requirements_changed(
        &mut self,
        generation: u64,
        objective_changed: bool,
        now: u64,
    ) -> Result<(), ControlError> {
        if generation <= self.required_input_generation
            || matches!(self.state, GoalState::Cancelled)
        {
            return Ok(());
        }
        self.required_input_generation = generation;
        for criterion in &mut self.criteria {
            if criterion.state == GoalCriterionState::Satisfied {
                criterion.state = GoalCriterionState::Stale;
                criterion.evidence_revision = None;
            }
        }
        if objective_changed {
            self.state = GoalState::Blocked;
            self.pause_mode = None;
            self.reason = ControlText::new(
                "The confirmed objective changed; clear or explicitly replace this goal."
                    .to_owned(),
            )?;
        } else if self.state == GoalState::Achieved {
            self.state = GoalState::Blocked;
            self.pause_mode = None;
            self.reason = ControlText::new(
                "Newer governing input invalidated prior completion evidence; explicitly resume."
                    .to_owned(),
            )?;
        }
        self.updated_unix_millis = now;
        Ok(())
    }
}
