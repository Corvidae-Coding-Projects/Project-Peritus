//! Goal construction, inspection, and user-controlled lifecycle transitions.

use super::{
    ControlError, GoalAttemptProgress, GoalCriterion, GoalCriterionKind,
    GoalCriterionState, GoalPauseMode, GoalRecord, GoalState, GoalUsage, OperationId,
};
use crate::control::GoalText;

impl GoalRecord {
    pub(in crate::control) fn start(
        id: OperationId,
        run: [u8; 16],
        objective: GoalText,
        criteria: Vec<GoalCriterion>,
        required_input_generation: u64,
        now: u64,
    ) -> Result<Self, ControlError> {
        if run == [0; 16]
            || required_input_generation == 0
            || criteria.is_empty()
            || !criteria.iter().any(|criterion| {
                criterion.mandatory && criterion.kind == GoalCriterionKind::RunnerAcceptance
            })
        {
            return Err(ControlError::InvalidInput);
        }
        let goal = Self {
            id,
            run,
            objective,
            criteria,
            state: GoalState::Active,
            reason: GoalText::new(
                "Goal confirmed; awaiting the next admitted operation.".to_owned(),
            )?,
            user_revision: 1,
            required_input_generation,
            attempt: 1,
            pause_mode: None,
            usage: GoalUsage::default(),
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
    /// Current cumulative usage and unresolved-reporting provenance.
    #[must_use]
    pub const fn usage(&self) -> &GoalUsage {
        &self.usage
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
        self.state == GoalState::Active
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
        self.state = if self.state == GoalState::WaitingForUser {
            GoalState::Paused
        } else {
            GoalState::Pausing
        };
        self.pause_mode = (self.state == GoalState::Pausing).then_some(mode);
        self.reason = GoalText::new(
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
        if !matches!(self.state, GoalState::Paused | GoalState::WaitingForUser | GoalState::Blocked)
        {
            return Err(ControlError::InvalidInput);
        }
        self.user_revision = self.user_revision.checked_add(1).ok_or(ControlError::Capacity)?;
        self.attempt = self.attempt.checked_add(1).ok_or(ControlError::Capacity)?;
        self.attempt_progress = GoalAttemptProgress::default();
        self.state = GoalState::Active;
        self.pause_mode = None;
        self.reason = GoalText::new(
            "Explicitly resumed with the same confirmed objective and cumulative accounting retained."
                .to_owned(),
        )?;
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
        self.reason = GoalText::new(
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
        let mut invalidated_evidence = false;
        for criterion in &mut self.criteria {
            if criterion.state == GoalCriterionState::Satisfied
                && criterion
                    .evidence_revision
                    .is_some_and(|revision| revision != generation)
            {
                criterion.state = GoalCriterionState::Stale;
                invalidated_evidence = true;
            }
        }
        if objective_changed {
            self.state = GoalState::Blocked;
            self.pause_mode = None;
            self.reason = GoalText::new(
                "The confirmed objective changed; clear or explicitly replace this goal."
                    .to_owned(),
            )?;
        } else if self.state == GoalState::Achieved && invalidated_evidence {
            self.state = GoalState::Blocked;
            self.pause_mode = None;
            self.reason = GoalText::new(
                "Newer governing input invalidated prior completion evidence; explicitly resume."
                    .to_owned(),
            )?;
        }
        self.updated_unix_millis = now;
        Ok(())
    }
}
