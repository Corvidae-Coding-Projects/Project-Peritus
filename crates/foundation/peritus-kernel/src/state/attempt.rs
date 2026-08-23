//! Attempt lifecycle state.

use peritus_budget::BudgetLimits;
use peritus_types::{AttemptId, BudgetId, RunId};
use vstd::prelude::*;

verus! {

/// Lifecycle phase of one run attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttemptPhase {
    /// Writer or fixer work may create turns.
    Active,
    /// The candidate was submitted for review.
    Submitted,
    /// Review activity is in progress.
    Reviewing,
    /// The candidate requires another fixer cycle.
    Fixing,
    /// Acceptance completed this attempt.
    Accepted,
    /// The attempt failed without accepting the run.
    Failed,
    /// The parent run cancelled this attempt.
    Cancelled,
    /// The attempt exhausted its governed budget.
    Exhausted,
}

impl AttemptPhase {
    /// Returns whether the attempt cannot resume.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted | Self::Failed | Self::Cancelled | Self::Exhausted)
    }
}

/// Current state of one attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AttemptState {
    id: AttemptId,
    run_id: RunId,
    budget_id: BudgetId,
    budget_limits: BudgetLimits,
    phase: AttemptPhase,
}

impl AttemptState {
    pub(crate) const fn active(
        id: AttemptId,
        run_id: RunId,
        budget_id: BudgetId,
        budget_limits: BudgetLimits,
    ) -> Self {
        Self { id, run_id, budget_id, budget_limits, phase: AttemptPhase::Active }
    }

    /// Returns the attempt identity.
    #[must_use]
    pub const fn id(self) -> AttemptId { self.id }
    /// Returns the parent run identity.
    #[must_use]
    pub const fn run_id(self) -> RunId { self.run_id }
    /// Returns the child budget identity.
    #[must_use]
    pub const fn budget_id(self) -> BudgetId { self.budget_id }
    /// Returns the immutable attempt limits.
    #[must_use]
    pub const fn budget_limits(self) -> BudgetLimits { self.budget_limits }
    /// Returns the current phase.
    #[must_use]
    pub const fn phase(self) -> AttemptPhase { self.phase }

    pub(crate) const fn set_phase(&mut self, phase: AttemptPhase) { self.phase = phase; }
}

} // verus!
