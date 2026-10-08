//! Authenticated execution discovery for a selected durable conversation.

use crate::{AppErrorCode, AppProtocolError, WorkbenchSnapshot};
use peritus_types::RunId;

/// Observed binding only; querying this state never starts or resumes execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchExecutionState {
    snapshot: WorkbenchSnapshot,
    run: Option<RunId>,
    goal: bool,
}
impl WorkbenchExecutionState {
    /// Binds current metadata to its optional original execution.
    ///
    /// # Errors
    /// Rejects a goal without its execution binding.
    pub fn new(
        snapshot: WorkbenchSnapshot,
        run: Option<RunId>,
        goal: bool,
    ) -> Result<Self, AppProtocolError> {
        if goal && run.is_none() {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { snapshot, run, goal })
    }
    /// Current authorized metadata and aggregate revision.
    #[must_use]
    pub const fn snapshot(&self) -> &WorkbenchSnapshot {
        &self.snapshot
    }
    /// Original execution, if one has been admitted.
    #[must_use]
    pub const fn run(&self) -> Option<RunId> {
        self.run
    }
    /// Whether explicit goal controls govern resumption.
    #[must_use]
    pub const fn has_goal(&self) -> bool {
        self.goal
    }
}

/// Explicit continuation semantics for an existing durable conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchContinuation {
    query: crate::WorkbenchQuery,
    mode: crate::ProductInteractionMode,
}
impl WorkbenchContinuation {
    /// Chooses the mode for the next execution without inventing an input.
    #[must_use]
    pub const fn new(query: crate::WorkbenchQuery, mode: crate::ProductInteractionMode) -> Self {
        Self { query, mode }
    }
    /// Exact authorized conversation scope.
    #[must_use]
    pub const fn query(self) -> crate::WorkbenchQuery {
        self.query
    }
    /// Requested execution mode; the host checks that work is idle before changing it.
    #[must_use]
    pub const fn mode(self) -> crate::ProductInteractionMode {
        self.mode
    }
}
/// Durable relationship between an accepted continuation and owned launch preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchContinuationAdmissionState {
    /// The control journal accepted the exact command, but no live or settled owner currently
    /// proves launch. Replaying the same command recovers its sealed admitted source.
    AcceptedPendingLaunch,
    /// A live owner names this exact operation, or its terminal ownership was durably settled.
    LaunchOwned,
}
impl WorkbenchContinuationAdmissionState {
    /// Stable canonical wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::AcceptedPendingLaunch => 1,
            Self::LaunchOwned => 2,
        }
    }
    /// Parses one stable canonical wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::AcceptedPendingLaunch),
            2 => Some(Self::LaunchOwned),
            _ => None,
        }
    }
}

/// Read-only exact continuation launch-admission observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchContinuationAdmission {
    operation: crate::ControlOperationId,
    query: crate::WorkbenchQuery,
    run: RunId,
    state: WorkbenchContinuationAdmissionState,
}
impl WorkbenchContinuationAdmission {
    /// Binds the exact accepted operation to its run-owned launch marker.
    #[must_use]
    pub const fn new(
        operation: crate::ControlOperationId,
        query: crate::WorkbenchQuery,
        run: RunId,
        state: WorkbenchContinuationAdmissionState,
    ) -> Self {
        Self { operation, query, run, state }
    }
    /// Original continuation operation identity.
    #[must_use]
    pub const fn operation(self) -> crate::ControlOperationId {
        self.operation
    }
    /// Exact conversation and workspace scope.
    #[must_use]
    pub const fn query(self) -> crate::WorkbenchQuery {
        self.query
    }
    /// Existing execution lineage.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }
    /// Whether durable run ownership has been recorded.
    #[must_use]
    pub const fn state(self) -> WorkbenchContinuationAdmissionState {
        self.state
    }
}
