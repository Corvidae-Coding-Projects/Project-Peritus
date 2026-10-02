//! Persistent-goal, safe-boundary, evidence, and usage DTOs.

use crate::{
    AppErrorCode, AppProtocolError, ControlOperationId, WorkbenchInputText, WorkbenchQuery,
};
use peritus_types::RunId;

/// Typed completion evidence requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGoalCriterionKind {
    /// Existing strict product-runner acceptance.
    RunnerAcceptance,
    /// Native graphical/playtest evidence.
    GraphicalPlaytest,
    /// Explicit human validation.
    HumanValidation,
}

/// One criterion in the user-confirmed definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalCriterionDefinition {
    kind: WorkbenchGoalCriterionKind,
    description: WorkbenchInputText,
    mandatory: bool,
}

impl WorkbenchGoalCriterionDefinition {
    /// Creates an exact typed criterion.
    #[must_use]
    pub const fn new(
        kind: WorkbenchGoalCriterionKind,
        description: WorkbenchInputText,
        mandatory: bool,
    ) -> Self {
        Self { kind, description, mandatory }
    }
    /// Criterion evidence kind.
    #[must_use]
    pub const fn kind(&self) -> WorkbenchGoalCriterionKind {
        self.kind
    }
    /// Exact displayed criterion text.
    #[must_use]
    pub const fn description(&self) -> &WorkbenchInputText {
        &self.description
    }
    /// Whether goal completion requires this criterion.
    #[must_use]
    pub const fn mandatory(&self) -> bool {
        self.mandatory
    }
}

/// Exact definition shown before confirmation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalDefinition {
    objective: WorkbenchInputText,
    criteria: Vec<WorkbenchGoalCriterionDefinition>,
}

impl WorkbenchGoalDefinition {
    /// Validates bounded criteria and requires mandatory strict runner acceptance.
    ///
    /// # Errors
    /// Rejects empty/unrepresentable criteria or a definition with no mandatory runner acceptance.
    pub fn new(
        objective: WorkbenchInputText,
        criteria: Vec<WorkbenchGoalCriterionDefinition>,
    ) -> Result<Self, AppProtocolError> {
        if criteria.is_empty()
            || u16::try_from(criteria.len()).is_err()
            || !criteria.iter().any(|criterion| {
                criterion.mandatory
                    && criterion.kind == WorkbenchGoalCriterionKind::RunnerAcceptance
            })
        {
            return Err(invalid());
        }
        Ok(Self { objective, criteria })
    }
    /// Exact user-confirmed objective.
    #[must_use]
    pub const fn objective(&self) -> &WorkbenchInputText {
        &self.objective
    }
    /// Typed completion criteria.
    #[must_use]
    pub fn criteria(&self) -> &[WorkbenchGoalCriterionDefinition] {
        &self.criteria
    }
}

/// Durable goal lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGoalState {
    /// Another bounded operation may be admitted.
    Active,
    /// Material user input or evidence is required.
    WaitingForUser,
    /// A durable pause awaits its selected safe boundary.
    Pausing,
    /// No further operation may start before explicit resume.
    Paused,
    /// Recovery or another prerequisite prevents continuation.
    Blocked,
    /// Every mandatory current criterion has admissible evidence.
    Achieved,
    /// Future continuation was explicitly cancelled.
    Cancelled,
}

/// Safe-boundary pause semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGoalPauseMode {
    /// Request cancellation of cancellable in-flight work.
    Now,
    /// Stop after the already-admitted operation settles.
    AfterOperation,
    /// Permit proven reads and stop before mutation-capable work.
    BeforeEdit,
}

/// Criterion evidence projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGoalCriterionState {
    /// No admissible current evidence exists.
    Pending,
    /// Exact current evidence satisfies the criterion.
    Satisfied,
    /// The installed phase cannot supply this evidence.
    Unavailable,
    /// A later governing input invalidated earlier evidence.
    Stale,
}

/// Current status of one exact criterion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalCriterion {
    definition: WorkbenchGoalCriterionDefinition,
    state: WorkbenchGoalCriterionState,
    evidence_revision: Option<u64>,
}

impl WorkbenchGoalCriterion {
    /// Creates one evidence projection.
    #[must_use]
    pub const fn new(
        definition: WorkbenchGoalCriterionDefinition,
        state: WorkbenchGoalCriterionState,
        evidence_revision: Option<u64>,
    ) -> Self {
        Self { definition, state, evidence_revision }
    }
    /// Immutable definition.
    #[must_use]
    pub const fn definition(&self) -> &WorkbenchGoalCriterionDefinition {
        &self.definition
    }
    /// Current evidence state.
    #[must_use]
    pub const fn state(&self) -> WorkbenchGoalCriterionState {
        self.state
    }
    /// Exact governing-input revision satisfied by evidence.
    #[must_use]
    pub const fn evidence_revision(&self) -> Option<u64> {
        self.evidence_revision
    }
}

mod projection;
pub use projection::*;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}
