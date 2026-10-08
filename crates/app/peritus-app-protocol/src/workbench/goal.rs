//! Persistent-goal, safe-boundary, evidence, and usage DTOs.

use crate::{
    AppErrorCode, AppProtocolError, ControlOperationId, WorkbenchInputText, WorkbenchQuery,
};
use peritus_types::RunId;

/// Exact inert goal text. Transport and storage enforce physical frame/page limits separately.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchGoalText(String);

impl WorkbenchGoalText {
    /// Validates nonempty inert goal text without imposing a product work quota.
    ///
    /// # Errors
    /// Rejects empty text and terminal controls other than newline and tab.
    pub fn new(text: String) -> Result<Self, AppProtocolError> {
        if text.trim().is_empty()
            || text.chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
        {
            return Err(invalid());
        }
        Ok(Self(text))
    }

    /// Borrows the exact accepted text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn legacy_wire_representable(&self) -> bool {
        self.0.len() <= crate::MAX_WORKBENCH_INPUT_BYTES
    }
}

impl From<WorkbenchInputText> for WorkbenchGoalText {
    fn from(value: WorkbenchInputText) -> Self {
        Self(value.as_str().to_owned())
    }
}

impl std::fmt::Debug for WorkbenchGoalText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkbenchGoalText")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

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
    description: WorkbenchGoalText,
    mandatory: bool,
}

impl WorkbenchGoalCriterionDefinition {
    /// Creates an exact typed criterion.
    #[must_use]
    pub fn new(
        kind: WorkbenchGoalCriterionKind,
        description: impl Into<WorkbenchGoalText>,
        mandatory: bool,
    ) -> Self {
        Self { kind, description: description.into(), mandatory }
    }
    /// Criterion evidence kind.
    #[must_use]
    pub const fn kind(&self) -> WorkbenchGoalCriterionKind {
        self.kind
    }
    /// Exact displayed criterion text.
    #[must_use]
    pub const fn description(&self) -> &WorkbenchGoalText {
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
    objective: WorkbenchGoalText,
    criteria: Vec<WorkbenchGoalCriterionDefinition>,
}

impl WorkbenchGoalDefinition {
    /// Validates bounded criteria and requires mandatory strict runner acceptance.
    ///
    /// # Errors
    /// Rejects empty/unrepresentable criteria or a definition with no mandatory runner acceptance.
    pub fn new(
        objective: impl Into<WorkbenchGoalText>,
        criteria: Vec<WorkbenchGoalCriterionDefinition>,
    ) -> Result<Self, AppProtocolError> {
        if criteria.is_empty()
            || !criteria.iter().any(|criterion| {
                criterion.mandatory
                    && criterion.kind == WorkbenchGoalCriterionKind::RunnerAcceptance
            })
        {
            return Err(invalid());
        }
        Ok(Self { objective: objective.into(), criteria })
    }
    /// Exact user-confirmed objective.
    #[must_use]
    pub const fn objective(&self) -> &WorkbenchGoalText {
        &self.objective
    }
    /// Typed completion criteria.
    #[must_use]
    pub fn criteria(&self) -> &[WorkbenchGoalCriterionDefinition] {
        &self.criteria
    }

    pub(crate) fn legacy_wire_representable(&self) -> bool {
        self.objective.legacy_wire_representable()
            && u16::try_from(self.criteria.len()).is_ok()
            && self.criteria.iter().all(|criterion| {
                criterion.description.legacy_wire_representable()
            })
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
