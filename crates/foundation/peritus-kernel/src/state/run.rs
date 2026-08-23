//! Run lifecycle state.

use crate::AcceptancePhase;
use peritus_budget::BudgetLimits;
use peritus_types::{AttemptId, BudgetId, RevisionTuple, RunId};
use vstd::prelude::*;

verus! {

/// Lifecycle phase of one governed coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RunPhase {
    /// Created and awaiting its first attempt.
    Pending,
    /// Writer/fixer work is active.
    Running,
    /// Work is explicitly paused.
    Paused,
    /// A submitted candidate is under review or acceptance evaluation.
    Reviewing,
    /// Current evidence requires a fixer cycle.
    Fixing,
    /// The exact current contract was satisfied.
    Accepted,
    /// The run was explicitly rejected.
    Rejected,
    /// The run was cancelled.
    Cancelled,
    /// The run failed.
    Failed,
    /// The run exhausted a governed budget.
    Exhausted,
}

impl RunPhase {
    /// Returns whether no later command may advance this run.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Rejected | Self::Cancelled | Self::Failed | Self::Exhausted
        )
    }
}

/// Current state of one governed run.
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
    pub(crate) proof fn equal_model_fields(left: Self, right: Self)
        requires left == right,
        ensures
            left.phase == right.phase,
            left.acceptance == right.acceptance,
    {}

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

    /// Returns the run identity.
    #[must_use]
    pub const fn id(self) -> RunId { self.id }
    /// Returns the exact revision governing the run.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple { self.revision }
    /// Returns the root run budget identity.
    #[must_use]
    pub const fn budget_id(self) -> BudgetId { self.budget_id }
    /// Returns the immutable run budget limits.
    #[must_use]
    pub const fn budget_limits(self) -> BudgetLimits { self.budget_limits }
    /// Returns the current run phase.
    #[must_use]
    pub const fn phase(self) -> RunPhase { self.phase }
    /// Returns the current acceptance phase.
    #[must_use]
    pub const fn acceptance(self) -> AcceptancePhase { self.acceptance }
    /// Returns the current attempt, if any.
    #[must_use]
    pub const fn current_attempt_id(self) -> Option<AttemptId> { self.current_attempt_id }

    pub(crate) const fn set_phase(&mut self, phase: RunPhase) { self.phase = phase; }
    pub(crate) const fn set_acceptance(&mut self, phase: AcceptancePhase) {
        self.acceptance = phase;
    }
    pub(crate) const fn set_current_attempt(&mut self, attempt_id: Option<AttemptId>) {
        self.current_attempt_id = attempt_id;
    }
}

} // verus!
