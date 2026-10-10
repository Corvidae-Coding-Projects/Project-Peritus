//! Run lifecycle state.

use crate::AcceptancePhase;
use peritus_budget::BudgetLimits;
use peritus_types::{AttemptId, BudgetId, RevisionTuple, RunId};

#[doc = " Lifecycle phase of one governed coding run."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RunPhase {
    #[doc = " Created and awaiting its first attempt."]
    Pending,
    #[doc = " Writer/fixer work is active."]
    Running,
    #[doc = " Work is explicitly paused."]
    Paused,
    #[doc = " A submitted candidate is under review or acceptance evaluation."]
    Reviewing,
    #[doc = " Current evidence requires a fixer cycle."]
    Fixing,
    #[doc = " The exact current contract was satisfied."]
    Accepted,
    #[doc = " The run was explicitly rejected."]
    Rejected,
    #[doc = " The run was cancelled."]
    Cancelled,
    #[doc = " The run failed."]
    Failed,
    #[doc = " The run exhausted a governed budget."]
    Exhausted,
}
impl RunPhase {
    #[doc = " Returns whether no later command may advance this run."]
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Rejected | Self::Cancelled | Self::Failed | Self::Exhausted
        )
    }
}
#[doc = " Current state of one governed run."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RunState {
    pub(crate) id: RunId,
    pub(crate) revision: RevisionTuple,
    pub(crate) budget_id: BudgetId,
    pub(crate) budget_limits: BudgetLimits,
    pub(crate) phase: RunPhase,
    pub(crate) acceptance: AcceptancePhase,
    pub(crate) current_attempt_id: Option<AttemptId>,
}
impl RunState {
    pub(crate) const fn pending(
        id: RunId,
        revision: RevisionTuple,
        budget_id: BudgetId,
        budget_limits: BudgetLimits,
    ) -> Self {
        Self {
            id,
            revision,
            budget_id,
            budget_limits,
            phase: RunPhase::Pending,
            acceptance: AcceptancePhase::Pending,
            current_attempt_id: None,
        }
    }
    #[doc = " Returns the run identity."]
    #[must_use]
    pub const fn id(self) -> RunId {
        self.id
    }
    #[doc = " Returns the exact revision governing the run."]
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the root run budget identity."]
    #[must_use]
    pub const fn budget_id(self) -> BudgetId {
        self.budget_id
    }
    #[doc = " Returns the immutable run budget limits."]
    #[must_use]
    pub const fn budget_limits(self) -> BudgetLimits {
        self.budget_limits
    }
    #[doc = " Returns the current run phase."]
    #[must_use]
    pub const fn phase(self) -> RunPhase {
        self.phase
    }
    #[doc = " Returns the current acceptance phase."]
    #[must_use]
    pub const fn acceptance(self) -> AcceptancePhase {
        self.acceptance
    }
    #[doc = " Returns the current attempt, if any."]
    #[must_use]
    pub const fn current_attempt_id(self) -> Option<AttemptId> {
        self.current_attempt_id
    }
    pub(crate) const fn set_phase(&mut self, phase: RunPhase) {
        self.phase = phase;
    }
    pub(crate) const fn set_acceptance(&mut self, phase: AcceptancePhase) {
        self.acceptance = phase;
    }
    pub(crate) const fn set_current_attempt(&mut self, attempt_id: Option<AttemptId>) {
        self.current_attempt_id = attempt_id;
    }
}
