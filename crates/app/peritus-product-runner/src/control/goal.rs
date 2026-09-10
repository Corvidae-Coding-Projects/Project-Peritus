//! Durable bounded-goal state, cumulative accounting, and safe-boundary admission.

use super::{ControlError, ControlText, OperationId};
use serde::Deserialize;
use serde::Serialize;

mod accounting;
mod execution;
mod lifecycle;
mod validation;

use accounting::GoalAttemptProgress;
pub use accounting::{
    GoalAdmission, GoalBudget, GoalRoleUsage, GoalSettlement, GoalUsage, GoalUsageReport,
};

#[cfg(test)]
mod tests;

const MAX_CRITERIA: usize = 16;

/// User-visible lifecycle of one logical goal, independent of an individual run attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    /// The existing product runner may admit the next bounded operation.
    Active,
    /// Further progress needs explicit user input or evidence.
    WaitingForUser,
    /// A durable pause request is waiting for its promised safe boundary.
    Pausing,
    /// No new provider or tool operation may start until explicit resume.
    Paused,
    /// Recovery or another concrete prerequisite prevents automatic continuation.
    Blocked,
    /// A cumulative limit prevents another operation from being admitted.
    BudgetReached,
    /// Every mandatory current criterion has independently supported evidence.
    Achieved,
    /// Future goal continuation was explicitly cancelled; history is retained.
    Cancelled,
}

/// Safe boundary requested by the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalPauseMode {
    /// Ask the owned provider/process cancellation mechanism to stop, without claiming rollback.
    Now,
    /// Let the already-admitted operation settle and stop before the next operation.
    AfterOperation,
    /// Permit proven read-only operations but stop before a mutation-capable operation.
    BeforeEdit,
}

/// Role attribution supplied locally at the exact D0 admission boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalRole {
    /// Designer/writer/conversational execution.
    Writer,
    /// Independent review execution.
    Reviewer,
    /// Review-remediation execution.
    Fixer,
}

impl GoalRole {
    const fn index(self) -> usize {
        match self {
            Self::Writer => 0,
            Self::Reviewer => 1,
            Self::Fixer => 2,
        }
    }
}

/// Typed completion criterion; unsupported evidence is never converted into success.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalCriterionKind {
    /// Existing strict runner settlement with current checkpoint/gate/review evidence.
    RunnerAcceptance,
    /// Native graphical launch/playtest evidence, supplied only by a later capability.
    GraphicalPlaytest,
    /// Explicit human validation; a model assertion is not human evidence.
    HumanValidation,
}

/// Current evidence state for one exact criterion revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalCriterionState {
    /// No admissible evidence has settled yet.
    Pending,
    /// Exact current evidence satisfies this criterion.
    Satisfied,
    /// The installed product phase cannot produce the required evidence.
    Unavailable,
    /// Earlier evidence was invalidated by a newer governing input revision.
    Stale,
}

/// One immutable goal criterion and its explicit evidence state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalCriterion {
    kind: GoalCriterionKind,
    description: ControlText<2048>,
    mandatory: bool,
    state: GoalCriterionState,
    evidence_revision: Option<u64>,
}

impl GoalCriterion {
    /// Creates an exact criterion. Graphical evidence starts visibly unavailable in P2.
    ///
    /// # Errors
    /// Rejects empty, oversized, or terminal-control-containing descriptions.
    pub fn new(
        kind: GoalCriterionKind,
        description: String,
        mandatory: bool,
    ) -> Result<Self, ControlError> {
        Ok(Self {
            kind,
            description: ControlText::new(description)?,
            mandatory,
            state: if kind == GoalCriterionKind::GraphicalPlaytest {
                GoalCriterionState::Unavailable
            } else {
                GoalCriterionState::Pending
            },
            evidence_revision: None,
        })
    }

    /// Returns the closed evidence kind.
    #[must_use]
    pub const fn kind(&self) -> GoalCriterionKind {
        self.kind
    }
    /// Borrows exact user-approved criterion text.
    #[must_use]
    pub fn description(&self) -> &str {
        self.description.as_str()
    }
    /// Returns whether completion requires this criterion.
    #[must_use]
    pub const fn mandatory(&self) -> bool {
        self.mandatory
    }
    /// Returns the current evidence state without inferring from model text.
    #[must_use]
    pub const fn state(&self) -> GoalCriterionState {
        self.state
    }
    /// Returns the exact governing input revision satisfied by current evidence.
    #[must_use]
    pub const fn evidence_revision(&self) -> Option<u64> {
        self.evidence_revision
    }
}

/// One current goal and its cumulative admission/evidence ledger.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalRecord {
    id: OperationId,
    run: [u8; 16],
    objective: ControlText<8192>,
    criteria: Vec<GoalCriterion>,
    budget: GoalBudget,
    state: GoalState,
    reason: ControlText<512>,
    user_revision: u64,
    required_input_generation: u64,
    attempt: u32,
    pause_mode: Option<GoalPauseMode>,
    usage: GoalUsage,
    #[serde(default)]
    child_budget_reservation: super::ChildBudgetReservation,
    attempt_progress: GoalAttemptProgress,
    created_unix_millis: u64,
    updated_unix_millis: u64,
}
