//! Independent state-transition relation used by generated trace tests.

use super::{Op, ReferenceModel};
use peritus_kernel::{
    AcceptancePhase, ActionPhase, AttemptPhase, ReviewPhase, RunPhase, SessionPhase, TurnPhase,
};

impl ReferenceModel {
    pub(super) const fn genesis() -> Self {
        Self {
            session: SessionPhase::Open,
            run: None,
            attempt: None,
            turn: None,
            action: None,
            review: None,
            acceptance: None,
            sequence: 1,
        }
    }

    pub(super) fn step(&mut self, op: Op) -> Result<(), ()> {
        self.apply(op)?;
        self.sequence += 1;
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one exhaustive match keeps the independent reference relation reviewable"
    )]
    fn apply(&mut self, op: Op) -> Result<(), ()> {
        match op {
            Op::PauseSession if self.session == SessionPhase::Open && self.run.is_none() => {
                self.session = SessionPhase::Paused;
            }
            Op::ResumeSession if self.session == SessionPhase::Paused => {
                self.session = SessionPhase::Open;
            }
            Op::StartRun if self.session == SessionPhase::Open && self.run.is_none() => {
                self.run = Some(RunPhase::Pending);
                self.acceptance = Some(AcceptancePhase::Pending);
            }
            Op::CancelRun => self.terminate_run(RunPhase::Cancelled)?,
            Op::FailRun => self.terminate_run(RunPhase::Failed)?,
            Op::ExhaustRun => self.terminate_run(RunPhase::Exhausted)?,
            Op::RejectRun => self.terminate_run(RunPhase::Rejected)?,
            Op::StartAttempt if self.run == Some(RunPhase::Pending) && self.attempt.is_none() => {
                self.run = Some(RunPhase::Running);
                self.attempt = Some(AttemptPhase::Active);
            }
            Op::FailAttempt => self.terminate_attempt(AttemptPhase::Failed)?,
            Op::ExhaustAttempt => self.terminate_attempt(AttemptPhase::Exhausted)?,
            Op::StartTurn if self.attempt == Some(AttemptPhase::Active) && self.turn.is_none() => {
                self.turn = Some(TurnPhase::Active);
            }
            Op::CancelTurn if self.turn == Some(TurnPhase::Active) => {
                self.turn = Some(TurnPhase::Cancelled);
                if matches!(self.action, Some(ActionPhase::Proposed | ActionPhase::Authorized)) {
                    self.action = Some(ActionPhase::Cancelled);
                }
            }
            Op::ProposeAction if self.turn == Some(TurnPhase::Active) && self.action.is_none() => {
                self.action = Some(ActionPhase::Proposed);
            }
            Op::AuthorizeAction if self.action == Some(ActionPhase::Proposed) => {
                self.action = Some(ActionPhase::Authorized);
            }
            Op::DispatchAction if self.action == Some(ActionPhase::Authorized) => {
                self.action = Some(ActionPhase::Dispatched);
            }
            Op::CompleteAction if self.action == Some(ActionPhase::Dispatched) => {
                self.action = Some(ActionPhase::Succeeded);
            }
            Op::CancelAction
                if matches!(self.action, Some(ActionPhase::Proposed | ActionPhase::Authorized)) =>
            {
                self.action = Some(ActionPhase::Cancelled);
            }
            Op::CompleteTurn
                if self.turn == Some(TurnPhase::Active)
                    && !matches!(
                        self.action,
                        Some(
                            ActionPhase::Proposed
                                | ActionPhase::Authorized
                                | ActionPhase::Dispatched
                        )
                    ) =>
            {
                self.turn = Some(TurnPhase::Completed);
            }
            Op::SubmitAttempt
                if self.run == Some(RunPhase::Running)
                    && self.attempt == Some(AttemptPhase::Active)
                    && self.turn != Some(TurnPhase::Active) =>
            {
                self.run = Some(RunPhase::Reviewing);
                self.attempt = Some(AttemptPhase::Submitted);
            }
            Op::RequestReview
                if self.run == Some(RunPhase::Reviewing)
                    && self.attempt == Some(AttemptPhase::Submitted)
                    && self.review.is_none() =>
            {
                self.attempt = Some(AttemptPhase::Reviewing);
                self.review = Some(ReviewPhase::Requested);
            }
            Op::BeginReview if self.review == Some(ReviewPhase::Requested) => {
                self.review = Some(ReviewPhase::Active);
            }
            Op::SubmitReview if self.review == Some(ReviewPhase::Active) => {
                self.review = Some(ReviewPhase::Submitted);
            }
            Op::BeginAcceptance
                if self.run == Some(RunPhase::Reviewing)
                    && self.attempt == Some(AttemptPhase::Reviewing)
                    && self.review == Some(ReviewPhase::Submitted)
                    && self.acceptance == Some(AcceptancePhase::Pending) =>
            {
                self.acceptance = Some(AcceptancePhase::Evaluating);
            }
            Op::EvaluateAcceptance { acceptable }
                if self.acceptance == Some(AcceptancePhase::Evaluating) =>
            {
                if acceptable {
                    self.run = Some(RunPhase::Accepted);
                    self.attempt = Some(AttemptPhase::Accepted);
                    self.acceptance = Some(AcceptancePhase::Accepted);
                } else {
                    self.run = Some(RunPhase::Fixing);
                    self.attempt = Some(AttemptPhase::Fixing);
                    self.acceptance = Some(AcceptancePhase::NeedsChanges);
                }
            }
            _ => return Err(()),
        }
        Ok(())
    }

    fn terminate_run(&mut self, phase: RunPhase) -> Result<(), ()> {
        if self.run.is_none_or(RunPhase::is_terminal) {
            return Err(());
        }
        self.run = Some(phase);
        self.acceptance = Some(AcceptancePhase::Terminated);
        self.attempt = self.attempt.map(|_| match phase {
            RunPhase::Cancelled => AttemptPhase::Cancelled,
            RunPhase::Exhausted => AttemptPhase::Exhausted,
            _ => AttemptPhase::Failed,
        });
        Ok(())
    }

    fn terminate_attempt(&mut self, phase: AttemptPhase) -> Result<(), ()> {
        if self.run != Some(RunPhase::Running) || self.attempt.is_none_or(AttemptPhase::is_terminal)
        {
            return Err(());
        }
        self.attempt = Some(phase);
        self.run = Some(RunPhase::Running);
        self.acceptance = Some(AcceptancePhase::Pending);
        Ok(())
    }
}
